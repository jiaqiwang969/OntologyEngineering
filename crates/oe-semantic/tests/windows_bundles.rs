//! Real immutable packages and Windows path identity; no Python or NX execution.
#![cfg(windows)]
use oe_semantic::load_bundle;
use std::path::Path;

#[test]
fn canonical_windows_roots_preserve_all_builtin_package_identities() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    for (name, expected) in [
        (
            "engineering-judgment-intake",
            "semantica.engineering.judgment-intake",
        ),
        (
            "engineering-evidence-methods",
            "semantica.engineering.evidence-methods",
        ),
        (
            "manufacturing-process-cost",
            "semantica.manufacturing.process-cost-loop",
        ),
    ] {
        let result = load_bundle(&root, name);
        assert!(
            result.is_ok(),
            "canonical Windows root could not load {name}: {:?}",
            result.err()
        );
        let loaded = result.unwrap();
        assert!(!loaded.payload.is_empty());
        assert_eq!(loaded.spec["package_id"].as_str().unwrap(), expected);
    }
}

#[test]
fn windows_bundle_hardlinks_are_rejected_with_unchanged_archive_bytes() {
    let original = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();
    let root = original.join(format!(
        "var/maintenance/windows-bundle-links-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(root.join("runtime/vendor")).unwrap();
    let archive = "runtime/vendor/engineering-judgment-intake-0.2.2.sem-package.zip";
    for name in [
        "runtime/semantic-bundles.json",
        "runtime/semantica-source-lock.json",
        archive,
    ] {
        std::fs::copy(original.join(name), root.join(name)).unwrap();
    }
    std::fs::hard_link(root.join(archive), root.join("alias.zip")).unwrap();
    assert_eq!(
        std::fs::read(root.join(archive)).unwrap(),
        std::fs::read(original.join(archive)).unwrap()
    );
    assert!(
        load_bundle(&root, "engineering-judgment-intake").is_err(),
        "hardlinked bundle was accepted"
    );
}
