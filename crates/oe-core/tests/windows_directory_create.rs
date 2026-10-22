#![cfg(windows)]
use oe_core::{SkillRoot, ensure_directory};
use oe_windows_controller::windows::StableRoot;
use std::{
    fs,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

static COUNT: AtomicU64 = AtomicU64::new(0);
fn scratch() -> std::path::PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    SkillRoot::new(root)
        .unwrap()
        .new_private_dir(Path::new(&format!(
            "var/maintenance/windows-directory-create/{}-{}",
            std::process::id(),
            COUNT.fetch_add(1, Ordering::Relaxed)
        )))
        .unwrap()
        .canonicalize()
        .unwrap()
}

#[test]
fn windows_directory_creation_preserves_existing_identity_and_bytes() {
    let scratch = scratch();
    let path = scratch.join("new/parent/leaf");
    ensure_directory(&path).expect("Windows must create nested pinned directories");
    let before = StableRoot::open(&path, None).unwrap();
    fs::write(path.join("kept"), b"unchanged\n").unwrap();
    ensure_directory(&path).unwrap();
    let after = StableRoot::open(&path, None).unwrap();
    assert_eq!(before.identity(), after.identity());
    assert_eq!(fs::read(path.join("kept")).unwrap(), b"unchanged\n");
}

#[test]
#[ignore = "requires an explicit task-private OEV2_WINDOWS_DIRECTORY_JUNCTION_FIXTURE prepared with native Windows tooling"]
fn windows_directory_creation_refuses_files_traversal_and_junctions() {
    use std::os::windows::fs::MetadataExt;
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let value = std::env::var_os("OEV2_WINDOWS_DIRECTORY_JUNCTION_FIXTURE")
        .expect("explicit native junction fixture required; missing setup is not a pass");
    let scratch = SkillRoot::new(root)
        .unwrap()
        .private_path(Path::new(&value))
        .unwrap()
        .canonicalize()
        .unwrap();
    assert_eq!(fs::read(scratch.join("regular")).unwrap(), b"keep\n");
    assert!(ensure_directory(&scratch.join("regular")).is_err());
    assert!(ensure_directory(&scratch.join("regular/child")).is_err());
    let ordinary =
        std::path::PathBuf::from(scratch.to_str().unwrap().strip_prefix(r"\\?\").unwrap());
    // A verbatim PathBuf::join removes '..' before this API sees it. Supply the
    // literal ordinary path and assert that the test really contains traversal.
    let traversal = ordinary.join("new/../escape");
    assert!(
        traversal
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    );
    assert!(ensure_directory(&traversal).is_err());
    assert!(!scratch.join("escape").exists());
    assert_eq!(fs::read(scratch.join("regular")).unwrap(), b"keep\n");
    let target = scratch.join("target");
    let alias = scratch.join("alias");
    assert!(target.is_dir());
    assert!(fs::read_dir(&target).unwrap().next().is_none());
    assert_ne!(
        fs::symlink_metadata(&alias).unwrap().file_attributes() & 0x400,
        0
    );
    assert!(ensure_directory(&alias).is_err());
    assert!(ensure_directory(&alias.join("created")).is_err());
    assert!(!target.join("created").exists());
    assert_eq!(fs::read(scratch.join("regular")).unwrap(), b"keep\n");
}
