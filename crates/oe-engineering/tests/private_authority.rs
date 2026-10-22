#![cfg(any(unix, windows))]
mod common;
use common::Fixture;
use oe_engineering::artifacts;
use serde_json::json;
use std::{fs, path::Path};

fn permissions(path: &Path, public: bool) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            path,
            fs::Permissions::from_mode(if public { 0o644 } else { 0o600 }),
        )
        .unwrap();
    }
    #[cfg(windows)]
    common::windows_authority::set_policy_acl(path, public);
}

#[test]
fn private_authority_preserves_exact_bytes_and_rechecks_current_content() {
    let f = Fixture::new("private-authority", "required_semantic_review");
    let path = f.root.path().join("var/inputs/authority.json");
    let bytes = b"{\"synthetic_rule\":true}\n";
    fs::write(&path, bytes).unwrap();
    permissions(&path, false);
    let (value, digest) = artifacts::private_policy(&f.root, "var/inputs/authority.json").unwrap();
    assert_eq!(value, json!({"synthetic_rule":true}));
    assert_eq!(digest, oe_core::sha256(bytes));
    assert_eq!(fs::read(&path).unwrap(), bytes);
    let replacement = b"{\"synthetic_rule\":false}";
    fs::write(&path, replacement).unwrap();
    let (value, changed) = artifacts::private_policy(&f.root, "var/inputs/authority.json").unwrap();
    assert_eq!(value, json!({"synthetic_rule":false}));
    assert_eq!(changed, oe_core::sha256(replacement));
    assert_ne!(changed, digest);
}

#[test]
fn private_authority_refuses_public_duplicate_oversized_and_outside_inputs() {
    let f = Fixture::new("private-authority-refusals", "required_semantic_review");
    let name = "var/inputs/authority.json";
    let path = f.root.path().join(name);
    fs::write(&path, b"{\"synthetic_rule\":true}").unwrap();
    permissions(&path, false);
    artifacts::private_policy(&f.root, name).unwrap();
    permissions(&path, true);
    assert!(artifacts::private_policy(&f.root, name).is_err());
    assert!(
        artifacts::private_policy(&f.root, name).is_err(),
        "reader must not repair the policy"
    );
    permissions(&path, false);
    fs::write(&path, b"{\"same\":1,\"same\":2}").unwrap();
    assert!(artifacts::private_policy(&f.root, name).is_err());
    fs::write(&path, vec![b' '; 1024 * 1024 + 1]).unwrap();
    assert!(artifacts::private_policy(&f.root, name).is_err());
    assert!(artifacts::private_policy(&f.root, "SKILL.md").is_err());
    assert!(artifacts::private_policy(&f.root, "var/../SKILL.md").is_err());
}
