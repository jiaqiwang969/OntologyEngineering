use oe_core::{create_new_private_dir, ensure_directory, rename_regular_file};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static SERIAL: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let path = root.join("var/maintenance/rust-tests").join(format!(
            "file-publish-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        create_new_private_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
}

#[cfg(unix)]
#[test]
fn regular_publication_replaces_one_name_and_preserves_other_hardlink_bytes() {
    let f = Fixture::new();
    let source = f.0.join("prepared");
    let target = f.0.join("target");
    let old_alias = f.0.join("old-alias");
    fs::write(&source, b"new contents\n").unwrap();
    rename_regular_file(&source, &target).unwrap();
    assert!(!source.exists());
    assert_eq!(fs::read(&target).unwrap(), b"new contents\n");
    fs::hard_link(&target, &old_alias).unwrap();
    fs::write(&source, b"next version\n").unwrap();
    rename_regular_file(&source, &target).unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"next version\n");
    assert_eq!(fs::read(&old_alias).unwrap(), b"new contents\n");
}

#[cfg(unix)]
#[test]
fn symlink_ancestor_leaf_and_nonregular_destination_cannot_redirect_publication() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let source = f.0.join("prepared");
    fs::write(&source, b"new\n").unwrap();
    fs::create_dir(f.0.join("other")).unwrap();
    fs::write(f.0.join("other/target"), b"original\n").unwrap();
    symlink(f.0.join("other"), f.0.join("alias")).unwrap();
    assert!(rename_regular_file(&source, &f.0.join("alias/target")).is_err());
    symlink(f.0.join("other/target"), f.0.join("leaf")).unwrap();
    assert!(rename_regular_file(&source, &f.0.join("leaf")).is_err());
    assert!(rename_regular_file(&source, &f.0.join("other")).is_err());
    assert_eq!(fs::read(f.0.join("other/target")).unwrap(), b"original\n");
    assert_eq!(fs::read(&source).unwrap(), b"new\n");
    assert!(rename_regular_file(&f.0.join("leaf"), &f.0.join("new-name")).is_err());
    assert!(!f.0.join("new-name").exists());
}

#[cfg(unix)]
#[test]
fn ensure_directory_creates_nested_paths_and_preserves_existing_directory_identity() {
    use std::os::unix::fs::MetadataExt;
    let f = Fixture::new();
    let path = f.0.join("new/parent/leaf");
    ensure_directory(&path).unwrap();
    let original = fs::metadata(&path).unwrap();
    fs::write(path.join("kept"), b"unchanged\n").unwrap();
    ensure_directory(&path).unwrap();
    let current = fs::metadata(&path).unwrap();
    assert!(current.is_dir());
    assert_eq!(
        (current.dev(), current.ino()),
        (original.dev(), original.ino())
    );
    assert_eq!(fs::read(path.join("kept")).unwrap(), b"unchanged\n");
}

#[cfg(unix)]
#[test]
fn ensure_directory_rejects_linked_parent_linked_leaf_and_regular_file() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let target = f.0.join("target");
    fs::create_dir(&target).unwrap();
    symlink(&target, f.0.join("alias")).unwrap();
    assert!(ensure_directory(&f.0.join("alias/created")).is_err());
    assert!(ensure_directory(&f.0.join("alias")).is_err());
    assert!(!target.join("created").exists());
    fs::write(f.0.join("regular"), b"keep\n").unwrap();
    assert!(ensure_directory(&f.0.join("regular")).is_err());
    assert!(ensure_directory(&f.0.join("regular/child")).is_err());
    assert_eq!(fs::read(f.0.join("regular")).unwrap(), b"keep\n");
}
