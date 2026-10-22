//! Deterministic coverage of the ancestor replacement interleaving.
use super::*;
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::symlink;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNT: AtomicU64 = AtomicU64::new(0);

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn fixture() -> (SkillRoot, Scratch) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let root = SkillRoot::new(path).unwrap();
    let scratch = root
        .new_private_dir(Path::new(&format!(
            "var/maintenance/rust-tests/directory-fd-{}-{}",
            std::process::id(),
            COUNT.fetch_add(1, Ordering::Relaxed)
        )))
        .unwrap();
    (root, Scratch(scratch))
}

#[test]
fn opened_parent_stays_bound_after_rename_and_symlink_replacement() {
    let (_, scratch) = fixture();
    let original = scratch.0.join("original");
    let parked = scratch.0.join("parked");
    let redirect = scratch.0.join("redirect");
    fs::create_dir(&original).unwrap();
    fs::create_dir(&redirect).unwrap();

    // Freeze the exact interleaving between directory traversal and creation.
    let held = unix_directory(&original, false).unwrap();
    fs::rename(&original, &parked).unwrap();
    symlink(&redirect, &original).unwrap();

    let name = unix_name(std::ffi::OsStr::new("evidence.txt")).unwrap();
    let descriptor = unsafe {
        libc::openat(
            held.as_raw_fd(),
            name.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o600,
        )
    };
    assert!(descriptor >= 0);
    let mut file = unsafe { fs::File::from_raw_fd(descriptor) };
    file.write_all(b"bound-to-opened-parent").unwrap();
    file.sync_all().unwrap();

    let child = unix_name(std::ffi::OsStr::new("child")).unwrap();
    assert_eq!(
        unsafe { libc::mkdirat(held.as_raw_fd(), child.as_ptr(), 0o700) },
        0
    );
    assert_eq!(
        fs::read(parked.join("evidence.txt")).unwrap(),
        b"bound-to-opened-parent"
    );
    assert!(parked.join("child").is_dir());
    assert_eq!(fs::read_dir(&redirect).unwrap().count(), 0);
}

#[test]
fn public_creators_reject_replaced_parent_and_leave_target_untouched() {
    let (root, scratch) = fixture();
    let original = scratch.0.join("original");
    let parked = scratch.0.join("parked");
    let redirect = scratch.0.join("redirect");
    fs::create_dir(&original).unwrap();
    fs::create_dir(&redirect).unwrap();
    let previously_checked = root.private_path(&original.join("evidence.txt")).unwrap();
    fs::rename(&original, &parked).unwrap();
    symlink(&redirect, &original).unwrap();

    assert!(write_new_bytes(&previously_checked, b"must-not-escape").is_err());
    assert!(root.new_private_dir(&original.join("child")).is_err());
    assert!(unix_directory(&original.join("new-parent"), true).is_err());
    assert_eq!(fs::read_dir(&redirect).unwrap().count(), 0);
    assert_eq!(fs::read_dir(&parked).unwrap().count(), 0);
}
