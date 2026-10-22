#![cfg(windows)]
use oe_contracts::{NxAssemblySourceClosure, NxAssemblySourceFile};
use oe_nx_protocol::{BlobIdentity, RelativePath};
use oe_nx_worker::motion_source::windows::copy_owned_source;
use oe_windows_controller::{adapter, windows::StableRoot};
use std::ffi::OsStr;
fn closure() -> NxAssemblySourceClosure {
    let file = |path: &str, b: &[u8]| NxAssemblySourceFile {
        path: path.into(),
        bytes: b.len() as u64,
        sha256: BlobIdentity::of(b).sha256.as_str().into(),
    };
    NxAssemblySourceClosure {
        schema: "oev2.nx-assembly-source-closure/1".into(),
        root_part: "root.prt".into(),
        files: vec![file("root.prt", b"ROOT"), file("sub/leaf.prt", b"LEAF")],
        max_total_bytes: 8,
    }
}
#[test]
fn owned_copy_preserves_tree_protects_inputs_and_allows_private_edits() {
    let temp = tempfile::tempdir().unwrap();
    let root = StableRoot::open(temp.path(), None).unwrap();
    let input = root.create_directory_new(OsStr::new("inputs")).unwrap();
    let out = root.create_directory_new(OsStr::new("out")).unwrap();
    input
        .create_file_new(OsStr::new("root.prt"), b"ROOT", 4)
        .unwrap();
    let sub = input.create_directory_new(OsStr::new("sub")).unwrap();
    sub.create_file_new(OsStr::new("leaf.prt"), b"LEAF", 4)
        .unwrap();
    let copy = copy_owned_source(&out, &closure(), |name, limit| {
        adapter::pin_relative_file(&input, &RelativePath::new(name)?, limit)
    })
    .unwrap();
    assert_eq!(std::fs::read(copy.root_part()).unwrap(), b"ROOT");
    assert_eq!(
        std::fs::read(copy.namespace().join("sub/leaf.prt")).unwrap(),
        b"LEAF"
    );
    assert!(
        std::fs::OpenOptions::new()
            .write(true)
            .open(input.path().join("root.prt"))
            .is_err()
    );
    assert!(std::fs::rename(copy.namespace(), out.path().join("moved")).is_err());
    std::fs::write(copy.root_part(), b"PRIVATE EDIT").unwrap();
    copy.verify().unwrap();
    assert_eq!(
        std::fs::read(input.path().join("root.prt")).unwrap(),
        b"ROOT"
    );
    assert!(
        copy_owned_source(&out, &closure(), |name, limit| adapter::pin_relative_file(
            &input,
            &RelativePath::new(name)?,
            limit
        ))
        .is_err()
    );
}
#[test]
fn changed_source_rejects_before_any_output_namespace_exists() {
    let temp = tempfile::tempdir().unwrap();
    let root = StableRoot::open(temp.path(), None).unwrap();
    let input = root.create_directory_new(OsStr::new("inputs")).unwrap();
    let out = root.create_directory_new(OsStr::new("out")).unwrap();
    input
        .create_file_new(OsStr::new("root.prt"), b"ROOT", 4)
        .unwrap();
    let sub = input.create_directory_new(OsStr::new("sub")).unwrap();
    sub.create_file_new(OsStr::new("leaf.prt"), b"FAIL", 4)
        .unwrap();
    assert!(
        copy_owned_source(&out, &closure(), |name, limit| adapter::pin_relative_file(
            &input,
            &RelativePath::new(name)?,
            limit
        ))
        .is_err()
    );
    assert!(!out.path().join("motion-assembly").exists());
}

#[test]
fn readonly_copy_verifier_detects_same_size_private_change() {
    let temp = tempfile::tempdir().unwrap();
    let root = StableRoot::open(temp.path(), None).unwrap();
    let input = root.create_directory_new(OsStr::new("inputs")).unwrap();
    let out = root.create_directory_new(OsStr::new("out")).unwrap();
    input
        .create_file_new(OsStr::new("root.prt"), b"ROOT", 4)
        .unwrap();
    let sub = input.create_directory_new(OsStr::new("sub")).unwrap();
    sub.create_file_new(OsStr::new("leaf.prt"), b"LEAF", 4)
        .unwrap();
    let copy = copy_owned_source(&out, &closure(), |name, limit| {
        adapter::pin_relative_file(&input, &RelativePath::new(name)?, limit)
    })
    .unwrap();
    copy.verify_copy_files().unwrap();
    std::fs::write(copy.namespace().join("sub/leaf.prt"), b"EDIT").unwrap();
    assert!(
        copy.verify_copy_files()
            .unwrap_err()
            .to_string()
            .contains("readonly_bytes_changed")
    );
    assert_eq!(std::fs::read(sub.path().join("leaf.prt")).unwrap(), b"LEAF");
    copy.verify().unwrap();
}
