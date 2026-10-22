//! Expectations bind the real full-library portable baseline, never a miniature fixture.
use oe_design::migration;
use serde_json::{Value, json};
use std::{fs, path::Path};

const SCOPE: &[u8] = include_bytes!("../../../runtime/semantica-scope.json");
const LOCK: &[u8] = include_bytes!("../../../runtime/semantica-scope-lock.json");

fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("runtime")).unwrap();
    fs::create_dir(root.path().join("specs")).unwrap();
    fs::write(root.path().join("runtime/semantica-scope.json"), SCOPE).unwrap();
    fs::write(root.path().join("runtime/semantica-scope-lock.json"), LOCK).unwrap();
    root
}
fn scope_change(root: &Path, change: impl FnOnce(&mut Value), resign: bool) {
    let mut scope: Value = serde_json::from_slice(SCOPE).unwrap();
    change(&mut scope);
    let raw = serde_json::to_vec_pretty(&scope).unwrap();
    fs::write(root.join("runtime/semantica-scope.json"), &raw).unwrap();
    if resign {
        let mut lock: Value = serde_json::from_slice(LOCK).unwrap();
        lock["scope_sha256"] = json!(oe_core::sha256(&raw));
        lock["scope_bytes"] = json!(raw.len());
        fs::write(
            root.join("runtime/semantica-scope-lock.json"),
            serde_json::to_vec_pretty(&lock).unwrap(),
        )
        .unwrap();
    }
}
fn assert_unverified(result: &Value) {
    assert_eq!(result["inventory_integrity"], "matched");
    assert_eq!(result["migration_status"], "incomplete");
    assert_eq!(result["behavior_status"], "unknown");
    assert!(result.get("completion_percent").is_none());
    assert!(result.get("complete").is_none());
    assert!(
        result["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["migration_status"] == "unknown")
    );
}

#[test]
fn full_original_scope_preserved_without_behavioral_claims() {
    let root = fixture();
    let r = migration::audit(root.path()).unwrap();
    assert_unverified(&r);
    for (key, count) in [
        ("tracked_files", 1469),
        ("package_python", 360),
        ("outside_python", 33),
        ("frontend_code", 80),
        ("public_symbols", 3915),
        ("public_classes", 862),
        ("public_functions", 607),
        ("public_methods", 2446),
        ("python_exports", 1206),
        ("frontend_exports", 288),
        ("routes", 87),
        ("original_test_symbols", 5268),
        ("test_files", 307),
        ("python_test_sources", 303),
        ("original_assertions", 7777),
        ("placeholder_candidates", 32),
        ("empty_modules", 1),
        ("empty_files", 15),
        ("console_entrypoints", 5),
    ] {
        assert_eq!(r["counts"][key], count, "{key}");
    }
    assert_eq!(r["identity"]["version"], "0.6.5+oe.7");
    assert_eq!(
        r["identity"]["commit"],
        "8b2ed123c380fc8abc78507e6e6648e6f5c0052c"
    );
    let source: Value = serde_json::from_slice(SCOPE).unwrap();
    assert_eq!(source["empty_modules"][0]["symbol"], "coming_soon");
    assert_eq!(source["empty_modules"][0]["declared_export_count"], 0);
    let text = String::from_utf8(SCOPE.to_vec()).unwrap();
    assert!(!text.contains("/Users/"));
    assert!(!text.contains("\"doc\":"));
    assert!(!text.contains("\"arguments\":"));
    assert!(
        source["frontend_code"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["path"] == "explorer/tests/graphStore.multi-edge.test.mjs")
    );
    assert!(
        source["frontend_code"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["path"] == "explorer/tests/pluginRegistry.temporal.test.mjs")
    );
}

#[test]
fn design_sources_only_create_declared_mappings() {
    let root = fixture();
    fs::write(
        root.path().join("specs/engineering.toml"),
        r#"
[[sources]]
id="legacy_full"
identity="Semantica0.6.5+oe.7 commit8b2ed123c380fc8abc78507e6e6648e6f5c0052c"
[[modules]]
id="semantic_runtime"
rust_path="crates/semantica-runtime/src/runtime.rs"
status="behavior_verified"
source_refs=["legacy_full"]
"#,
    )
    .unwrap();
    let r = migration::audit(root.path()).unwrap();
    assert_unverified(&r);
    assert_eq!(r["design_mappings"][0]["status"], "declared");
    assert_eq!(
        r["design_mappings"][0]["claimed_status"],
        "behavior_verified"
    );
    assert_eq!(r["design_mappings"][0]["behavior_status"], "unknown");
}

macro_rules! deletion_case {
    ($name:ident,$section:literal) => {
        #[test]
        fn $name() {
            let root = fixture();
            scope_change(
                root.path(),
                |s| {
                    s[$section].as_array_mut().unwrap().pop();
                },
                false,
            );
            assert!(migration::audit(root.path()).is_err());
        }
    };
}
deletion_case!(removed_package_source_is_rejected, "package_python");
deletion_case!(removed_public_api_is_rejected, "public_symbols");
deletion_case!(removed_frontend_is_rejected, "frontend_code");
deletion_case!(removed_route_is_rejected, "routes");
deletion_case!(removed_original_test_is_rejected, "original_test_symbols");
deletion_case!(removed_tracked_identity_is_rejected, "tracked_files");

#[test]
fn re_signing_reduced_scope_cannot_replace_frozen_baseline() {
    let root = fixture();
    scope_change(
        root.path(),
        |s| {
            s["public_symbols"].as_array_mut().unwrap().pop();
            s["counts"]["public_symbols"] = json!(3914);
        },
        true,
    );
    assert!(migration::audit(root.path()).is_err());
}
#[test]
fn changed_source_sha_is_rejected() {
    let root = fixture();
    scope_change(
        root.path(),
        |s| s["tracked_files"][0]["sha256"] = json!("0".repeat(64)),
        false,
    );
    assert!(migration::audit(root.path()).is_err());
}
#[test]
fn duplicate_stable_api_id_is_rejected() {
    let root = fixture();
    scope_change(
        root.path(),
        |s| {
            s["public_symbols"][1] = s["public_symbols"][0].clone();
        },
        true,
    );
    assert!(migration::audit(root.path()).is_err());
}
#[test]
fn unsafe_relative_path_is_rejected() {
    for path in [
        "../private",
        "/Users/private",
        "a/../../b",
        "a\\b",
        "a//b",
        "C:private",
        "a\nsecret",
    ] {
        let root = fixture();
        scope_change(
            root.path(),
            |s| s["tracked_files"][0]["path"] = json!(path),
            true,
        );
        assert!(migration::audit(root.path()).is_err(), "{path:?}");
    }
}
#[test]
fn fake_verified_or_complete_inventory_is_rejected() {
    for status in ["verified", "complete", "full", "behavior_verified"] {
        let root = fixture();
        scope_change(root.path(), |s| s["migration_status"] = json!(status), true);
        assert!(migration::audit(root.path()).is_err());
    }
}
#[test]
fn duplicate_json_key_cannot_hide_scope_status() {
    let root = fixture();
    let mut raw = SCOPE.to_vec();
    let at = raw.iter().position(|b| *b == b'{').unwrap() + 1;
    raw.splice(
        at..at,
        b"\"migration_status\":\"complete\",".iter().copied(),
    );
    fs::write(root.path().join("runtime/semantica-scope.json"), raw).unwrap();
    assert!(migration::audit(root.path()).is_err());
}
#[test]
fn explicit_api_mapping_does_not_verify_behavior() {
    let root = fixture();
    let scope: Value = serde_json::from_slice(SCOPE).unwrap();
    let mapping = json!({"schema":"semantica.migration-mappings/1","scope_sha256":oe_core::sha256(SCOPE),"claims":[{"scope_ids":[scope["public_symbols"][0]["id"].clone()],"module":"semantic_runtime","rust_path":"crates/semantica-runtime/src/runtime.rs","status":"declared"}]});
    fs::write(
        root.path()
            .join("runtime/semantica-migration-mappings.json"),
        serde_json::to_vec_pretty(&mapping).unwrap(),
    )
    .unwrap();
    let r = migration::audit(root.path()).unwrap();
    assert_unverified(&r);
    assert_eq!(r["declared_mappings"][0]["status"], "declared");
    assert_eq!(r["declared_mappings"][0]["behavior_status"], "unknown");
}
#[test]
fn forged_verified_mapping_and_unsafe_mapping_path_are_rejected() {
    let scope: Value = serde_json::from_slice(SCOPE).unwrap();
    for (status, path) in [
        ("verified", "crates/x/src/lib.rs"),
        ("full", "crates/x/src/lib.rs"),
        ("declared", "../secret"),
    ] {
        let root = fixture();
        let mapping = json!({"schema":"semantica.migration-mappings/1","scope_sha256":oe_core::sha256(SCOPE),"claims":[{"scope_ids":[scope["public_symbols"][0]["id"].clone()],"module":"semantic_runtime","rust_path":path,"status":status}]});
        fs::write(
            root.path()
                .join("runtime/semantica-migration-mappings.json"),
            serde_json::to_vec_pretty(&mapping).unwrap(),
        )
        .unwrap();
        assert!(migration::audit(root.path()).is_err());
    }
}

#[test]
fn original_empty_files_and_console_exports_remain_unknown() {
    let root = fixture();
    let r = migration::audit(root.path()).unwrap();
    assert_unverified(&r);
    let items = r["items"].as_array().unwrap();
    let empty: Vec<_> = items
        .iter()
        .filter(|x| x["kind"] == "original_empty_file")
        .collect();
    assert_eq!(empty.len(), 15);
    assert!(empty.iter().all(|x| x["bytes"] == 0));
    assert!(empty.iter().any(|x| x["path"] == "explorer/src/App.css"));
    let console: Vec<_> = items
        .iter()
        .filter(|x| x["kind"] == "console_entrypoint")
        .collect();
    assert_eq!(console.len(), 5);
    assert!(
        console
            .iter()
            .any(|x| x["symbol"] == "semantica-worker" && x["target"] == "semantica.worker:main")
    );
}

deletion_case!(
    removed_console_entrypoint_is_rejected,
    "console_entrypoints"
);
deletion_case!(removed_empty_file_identity_is_rejected, "empty_files");

#[test]
fn invalid_scope_id_mapping_cannot_invent_new_surface() {
    let root = fixture();
    let mapping = json!({"schema":"semantica.migration-mappings/1","scope_sha256":oe_core::sha256(SCOPE),"claims":[{"scope_ids":["imaginary|function|invented|1"],"module":"semantic_runtime","rust_path":"crates/semantica-runtime/src/runtime.rs","status":"declared"}]});
    fs::write(
        root.path()
            .join("runtime/semantica-migration-mappings.json"),
        serde_json::to_vec_pretty(&mapping).unwrap(),
    )
    .unwrap();
    assert!(migration::audit(root.path()).is_err());
}

#[cfg(unix)]
#[test]
fn symlink_cannot_substitute_a_frozen_scope_or_design_directory() {
    use std::os::unix::fs::symlink;
    let root = fixture();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("scope.json"), SCOPE).unwrap();
    let path = root.path().join("runtime/semantica-scope.json");
    fs::remove_file(&path).unwrap();
    symlink(outside.path().join("scope.json"), &path).unwrap();
    assert!(migration::audit(root.path()).is_err());
    let root = fixture();
    fs::remove_dir(root.path().join("specs")).unwrap();
    fs::write(outside.path().join("engineering.toml"), "").unwrap();
    symlink(outside.path(), root.path().join("specs")).unwrap();
    assert!(migration::audit(root.path()).is_err());
}
