//! Actual shared capture probe preserves legacy behavior and rejects links.
use oe_design::composition::transport::{FILE_BYTES, current_capture};
use std::fs;

#[test]
fn composition_capture_legacy_probe_preserves_caps_and_rejects_links() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    assert!(current_capture(&root).unwrap().is_empty());
    fs::create_dir(root.join("specs")).unwrap();
    let source = root.join("specs/engineering.toml");
    fs::write(&source, b"not valid TOML {\n").unwrap();
    assert!(current_capture(&root).unwrap().is_empty());
    fs::write(&source, b"schema='oev2.design/1'\n").unwrap();
    assert!(current_capture(&root).unwrap().is_empty());
    fs::write(&source, vec![b' '; FILE_BYTES + 1]).unwrap();
    assert!(current_capture(&root).unwrap().is_empty());
    assert!(transport_read(&source).unwrap_err().contains("byte_budget"));
    fs::remove_file(&source).unwrap();
    fs::create_dir(&source).unwrap();
    assert!(current_capture(&root).is_err());
    fs::remove_dir(&source).unwrap();
    #[cfg(unix)]
    {
        // The missing target must not turn a dangling link into empty capture.
        std::os::unix::fs::symlink(root.join("missing"), &source).unwrap();
        assert!(current_capture(&root).is_err());
        fs::remove_file(&source).unwrap();
        fs::write(root.join("real"), b"schema='oev2.design/1'\n").unwrap();
        std::os::unix::fs::symlink(root.join("real"), &source).unwrap();
        assert!(current_capture(&root).is_err());
    }
}

fn transport_read(path: &std::path::Path) -> Result<Vec<u8>, String> {
    oe_design::composition::transport::read(path)
}
