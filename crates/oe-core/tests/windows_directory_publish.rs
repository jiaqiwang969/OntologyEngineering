#![cfg(windows)]
use oe_core::rename_directory_exclusive;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
fn scratch() -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../var/maintenance/windows-directory-publish-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&path).unwrap();
    path.canonicalize().unwrap()
}
#[test]
fn windows_directory_publication_moves_exact_source_to_absent_destination() {
    let root = scratch();
    let from = root.join("source");
    let to = root.join("destination");
    std::fs::create_dir(&from).unwrap();
    std::fs::write(from.join("member"), b"immutable source").unwrap();
    rename_directory_exclusive(&from, &to).unwrap();
    assert!(!from.exists());
    assert_eq!(
        std::fs::read(to.join("member")).unwrap(),
        b"immutable source"
    );
}
#[test]
fn windows_directory_publication_preserves_existing_empty_and_nonempty_targets() {
    for empty in [true, false] {
        let root = scratch();
        let from = root.join("source");
        let to = root.join("destination");
        std::fs::create_dir(&from).unwrap();
        std::fs::create_dir(&to).unwrap();
        std::fs::write(from.join("member"), b"new source").unwrap();
        if !empty {
            std::fs::write(to.join("member"), b"previous target").unwrap();
        }
        assert!(rename_directory_exclusive(&from, &to).is_err());
        assert_eq!(std::fs::read(from.join("member")).unwrap(), b"new source");
        if empty {
            assert!(std::fs::read_dir(&to).unwrap().next().is_none());
        } else {
            assert_eq!(
                std::fs::read(to.join("member")).unwrap(),
                b"previous target"
            );
        }
    }
}
#[test]
fn windows_directory_publication_refuses_a_file_source() {
    let root = scratch();
    let from = root.join("source");
    let to = root.join("destination");
    std::fs::write(&from, b"regular file").unwrap();
    assert!(rename_directory_exclusive(&from, &to).is_err());
    assert_eq!(std::fs::read(&from).unwrap(), b"regular file");
    assert!(!to.exists());
}
