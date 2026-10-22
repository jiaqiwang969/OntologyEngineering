use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

struct Temporary(PathBuf);
impl Temporary {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let base = std::env::temp_dir().canonicalize().unwrap();
        let path = base.join(format!(
            "oe-hash-identity-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn streamed_block_boundaries_match_independent_fixed_digests() {
    // Fixed independently before changing the SHA2 dependency. Each input byte
    // is (index * 37 + 11) modulo 256; values include both SHA padding boundaries
    // and the file reader's 64 KiB boundary.
    let vectors: &[(usize, &str)] = &[
        (
            1,
            "e7cf46a078fed4fafd0b5e3aff144802b853f8ae459a4f0c14add3314b7cc3a6",
        ),
        (
            55,
            "2900465fcb533e05a158fd2b3be0e5e3b03740d83060aa3580e0d98a96bf2384",
        ),
        (
            56,
            "31454ff48ef36af2f08fd511bdc37d9d5855ac23e992e5ff5445cb6b7674a674",
        ),
        (
            63,
            "5f6401b96532c36de4e65beec0409b69b1d181864c8009b7a04f43e5d56350d1",
        ),
        (
            64,
            "94eb5de4943613fd048dc93393ab06877405faa39c11f53e9386083339833e7e",
        ),
        (
            65,
            "fc518669b6eb4b4dd91827ecacef86689c725bd5bab888fd3b26dbb196eec954",
        ),
        (
            127,
            "0fe729ff19257bd6fec853acc2ea355f6b34b58e6c0f684c3e188fcdfcd9baae",
        ),
        (
            128,
            "0aedd4856f8eba0963627336ad5144a9a7dbe12498e6066f0165fc97d8ddee4c",
        ),
        (
            129,
            "4f1757ae4bffbae86d775b831765b75af154d52f7deaa46dd378051a2d3ad57f",
        ),
        (
            65535,
            "11cad04d25d7e2ae2bde1131397c1ee623dcf0731e3e123f278fa192a9d72976",
        ),
        (
            65536,
            "6fc179cfd193754e6109ad043f56d146c7e7d7c3623ffceae318266286f58388",
        ),
        (
            65537,
            "3123afca912e156e19bcd2055eb92e944cf6e6fdb0eef16d65464342ad1f5a1a",
        ),
        (
            1048579,
            "6c193945001535a236c79ce70fae7e6a7ad4ba24c7ac6c79cebc134b0c2359d2",
        ),
    ];
    let temp = Temporary::new();
    for &(length, expected) in vectors {
        let path = temp.path(&length.to_string());
        let raw: Vec<u8> = (0..length).map(|i| ((i * 37 + 11) % 256) as u8).collect();
        fs::write(&path, raw).unwrap();
        let identity = oe_core::fingerprint_file(&path, length as u64).unwrap();
        assert_eq!(identity.bytes, length as u64);
        assert_eq!(identity.sha256, expected);
    }
}

#[test]
fn hash_limit_keeps_exact_empty_and_nonempty_bounds() {
    let temp = Temporary::new();
    let empty = temp.path("empty");
    fs::write(&empty, []).unwrap();
    let id = oe_core::fingerprint_file(&empty, 0).unwrap();
    assert_eq!(id.bytes, 0);
    assert_eq!(
        id.sha256,
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    let path = temp.path("abc");
    fs::write(&path, b"abc").unwrap();
    assert!(oe_core::fingerprint_file(&path, 2).is_err());
    assert_eq!(
        oe_core::fingerprint_file(&path, 3).unwrap().sha256,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn same_length_and_mtime_do_not_replace_content_identity() {
    let temp = Temporary::new();
    let path = temp.path("mutable");
    fs::write(&path, b"ABCD").unwrap();
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    let first = oe_core::fingerprint_file(&path, 4).unwrap();
    fs::write(&path, b"EBCD").unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(modified))
        .unwrap();
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified);
    let second = oe_core::fingerprint_file(&path, 4).unwrap();
    assert_eq!(first.bytes, second.bytes);
    assert_ne!(first.sha256, second.sha256);
}

#[test]
fn directory_is_not_a_hashable_regular_file() {
    let temp = Temporary::new();
    assert!(oe_core::fingerprint_file(Path::new(&temp.0), 0).is_err());
}

#[test]
#[cfg(unix)]
fn leaf_and_ancestor_symlinks_do_not_authorize_hashing() {
    use std::os::unix::fs::symlink;
    let temp = Temporary::new();
    let dir = temp.path("original");
    fs::create_dir(&dir).unwrap();
    let file = dir.join("file");
    fs::write(&file, b"abc").unwrap();
    let leaf = temp.path("leaf");
    symlink(&file, &leaf).unwrap();
    let ancestor = temp.path("ancestor");
    symlink(&dir, &ancestor).unwrap();
    assert!(oe_core::fingerprint_file(&leaf, 3).is_err());
    assert!(oe_core::fingerprint_file(&ancestor.join("file"), 3).is_err());
}

#[test]
#[cfg(unix)]
fn fifo_is_rejected_without_waiting_for_a_writer() {
    use std::os::unix::ffi::OsStrExt;
    let temp = Temporary::new();
    let path = temp.path("pipe");
    let cpath = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(cpath.as_ptr(), 0o600) }, 0);
    assert!(oe_core::fingerprint_file(&path, 1024).is_err());
}
