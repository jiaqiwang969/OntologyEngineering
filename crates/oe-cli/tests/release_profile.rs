//! Actual CLI/source binding and dependency observations; no model/native calls.
use serde_json::Value;
use std::path::Path;
use std::process::Command;

fn command(profile: &str) -> std::process::Output {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    Command::new(env!("CARGO_BIN_EXE_oev2"))
        .args([
            "--root",
            root.to_str().unwrap(),
            "dist",
            "release",
            "check-profile",
            "--profile",
            profile,
        ])
        .output()
        .unwrap()
}

#[test]
fn actual_cli_reports_transitive_features_without_granting_behavior() {
    let output = command("oev2.rc5-local-source/1");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["profile"]["version"], "2.0.0-rc.5");
    assert_eq!(
        value["profile"]["compiled_semantic_features"],
        serde_json::json!([
            "CARGO_FEATURE_RDFLIB760_NULLABLE_INTERNAL",
            "CARGO_FEATURE_SOURCE_SESSION_LOCAL_V1"
        ])
    );
    assert_eq!(value["behavioral_qualification"], "not_evaluated");
    assert_eq!(value["formal_selection"], "unchanged");
    assert_eq!(value["engineering_adoption"], "not_granted");
    assert_eq!(
        value["execution_binding"]["executable_sha256"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
}

#[test]
fn actual_cli_cannot_claim_old_or_unknown_profile() {
    for profile in ["oev2.rc4-feature-off/1", "oev2.rc5-local-source/2"] {
        let output = command(profile);
        assert_eq!(output.status.code(), Some(2));
        assert!(!String::from_utf8_lossy(&output.stdout).contains("compiled_profile_matches"));
    }
}
