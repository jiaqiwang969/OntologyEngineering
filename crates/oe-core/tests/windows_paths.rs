#![cfg(windows)]
use oe_core::SkillRoot;
use std::path::{Path, PathBuf};

#[test]
fn ordinary_and_verbatim_windows_paths_preserve_the_same_private_root() {
    let canonical = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let root = SkillRoot::new(&canonical).unwrap();
    let ordinary = PathBuf::from(canonical.to_str().unwrap().strip_prefix(r"\\?\").unwrap());
    let relative = Path::new("var/maintenance/path-identity/new-output.json");
    let expected = root.private_path(relative).unwrap();
    let actual = root.private_path(&ordinary.join(relative));
    assert!(
        actual.is_ok(),
        "same Windows root was rejected: {:?}",
        actual.err()
    );
    assert_eq!(actual.unwrap(), expected);
    assert!(
        root.private_path(&ordinary.join("docs/forbidden-output.json"))
            .is_err()
    );
    assert!(
        root.private_path(Path::new("var/../docs/forbidden-output.json"))
            .is_err()
    );
}

#[test]
fn explicit_windows_parent_traversal_is_rejected_before_verbatim_join() {
    let canonical = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let root = SkillRoot::new(canonical).unwrap();
    let value = Path::new("var/unused-parent/../still-private.json");
    assert!(
        value
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    );
    assert!(
        root.private_path(value).is_err(),
        "explicit traversal was normalized away"
    );
}
