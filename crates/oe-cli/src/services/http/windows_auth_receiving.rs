//! Actual Windows controls; no platform skip or weaker substitute for Unix tests.
use super::{HostAuthorization, authorize};
use std::path::Path;

#[path = "../../../tests/support/http_windows_fixture.rs"]
mod fixture;

fn granted(path: &Path, key: &str) -> bool {
    matches!(
        authorize(Some(path), false, Some(key)),
        HostAuthorization::Granted
    )
}
fn unavailable(path: &Path) -> bool {
    matches!(
        authorize(Some(path), false, Some("synthetic-key-one")),
        HostAuthorization::NotConfigured
    )
}

#[test]
fn windows_authentication_accepts_fresh_private_key_and_rotation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("key");
    assert!(matches!(
        authorize(None, false, None),
        HostAuthorization::NotConfigured
    ));
    assert!(matches!(
        authorize(None, true, None),
        HostAuthorization::Granted
    ));
    std::fs::write(&path, "synthetic-key-one").unwrap();
    fixture::private(&path);
    // Independently establish the actual private-file prerequisite before
    // checking the HTTP adapter; always-NotConfigured must not pass this case.
    assert_eq!(
        oe_core::private_file::read(&path, 4096).unwrap().bytes,
        b"synthetic-key-one"
    );
    assert!(granted(&path, "synthetic-key-one"));
    assert!(matches!(
        authorize(Some(&path), false, None),
        HostAuthorization::Unauthorized
    ));
    assert!(matches!(
        authorize(Some(&path), false, Some("wrong")),
        HostAuthorization::Unauthorized
    ));
    std::fs::write(&path, "synthetic-key-two").unwrap();
    assert!(matches!(
        authorize(Some(&path), false, Some("synthetic-key-one")),
        HostAuthorization::Unauthorized
    ));
    assert!(granted(&path, "synthetic-key-two"));
}

#[test]
fn windows_authentication_refuses_unsafe_files_and_recovers() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("key");
    std::fs::write(&path, "synthetic-key-one").unwrap();
    fixture::private(&path);
    assert!(granted(&path, "synthetic-key-one"));
    for write in [false, true] {
        fixture::grant_other(&path, write);
        assert!(unavailable(&path));
        fixture::private(&path);
        assert!(granted(&path, "synthetic-key-one"));
    }
    for bytes in [
        Vec::new(),
        b"has whitespace".to_vec(),
        vec![0xff],
        vec![b'x'; 4097],
    ] {
        std::fs::write(&path, bytes).unwrap();
        assert!(unavailable(&path));
    }
    std::fs::write(&path, "synthetic-key-one").unwrap();
    let hard = dir.path().join("hard");
    std::fs::hard_link(&path, &hard).unwrap();
    assert!(unavailable(&path));
    std::fs::remove_file(hard).unwrap();
    let link = dir.path().join("link");
    std::os::windows::fs::symlink_file(&path, &link).unwrap();
    assert!(unavailable(&link));
    std::fs::remove_file(link).unwrap();
    let parent_link = dir.path().join("parent-link");
    std::os::windows::fs::symlink_dir(dir.path(), &parent_link).unwrap();
    assert!(unavailable(&parent_link.join("key")));
    std::fs::remove_dir(parent_link).unwrap();
    assert!(unavailable(dir.path()));
    assert!(unavailable(&dir.path().join("missing")));
    assert!(granted(&path, "synthetic-key-one"));
}

#[test]
fn windows_authentication_observes_same_path_replacement() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("key");
    std::fs::write(&path, "synthetic-key-one").unwrap();
    fixture::private(&path);
    let snapshot = oe_core::private_file::read(&path, 4096).unwrap();
    assert!(granted(&path, "synthetic-key-one"));
    let replacement = dir.path().join("replacement");
    std::fs::write(&replacement, "synthetic-key-two").unwrap();
    fixture::private(&replacement);
    std::fs::remove_file(&path).unwrap();
    std::fs::rename(replacement, &path).unwrap();
    assert!(snapshot.recheck(&path, 4096).is_err());
    assert!(matches!(
        authorize(Some(&path), false, Some("synthetic-key-one")),
        HostAuthorization::Unauthorized
    ));
    assert!(granted(&path, "synthetic-key-two"));
}
