//! The real CLI must bind the code it executes, and report safe stage failures.
use oe_core::sha256;
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};

fn source_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}
fn snapshot() -> tempfile::TempDir {
    let source = source_root();
    let parent = source.join("var/maintenance/harness-source-binding-tests");
    fs::create_dir_all(&parent).unwrap();
    let target = tempfile::tempdir_in(parent).unwrap();
    let manifest = oe_design::provenance::compiled().unwrap();
    let mut files: Vec<String> = manifest["files"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    files.extend(
        [
            "SKILL.md",
            "generated/contracts.rs",
            "generated/semantic-contracts.rs",
            "references/context-capabilities.json",
            "references/context-routing-instructions.json",
        ]
        .map(str::to_owned),
    );
    for relative in files {
        let path = target.path().join(&relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::copy(source.join(relative), path).unwrap();
    }
    target
}
fn input(root: &Path, conditions: Value) -> std::path::PathBuf {
    let value = json!({"schema":"ontology-engineering.context-input/v1","task_id":"source-bound",
        "context":"Understand a mechanism with unknown support.","request":"Preserve necessary checks.",
        "harness":{"project_id":"synthetic-project","decision_id":"synthetic-decision","object_id":"synthetic-object",
        "revision":"r1","problem_class":"source-bound-check","goal":"current implementation only",
        "conditions":conditions,"constraints":["support required"],"unknowns":["holding force unknown"],
        "semantic_modules":[],"review_requests":[],"observations":[]}});
    let path = root.join("var/input.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    path
}
fn run(root: &Path, input: &Path, name: &str) -> (i32, Value, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_oev2"))
        .env_clear()
        .env("PATH", "")
        .arg("--root")
        .arg(root)
        .args(["harness", "step", "--input"])
        .arg(input)
        .arg("--output")
        .arg(root.join("var").join(name))
        .arg("--credential-file")
        .arg(root.join("absent-credential.json"))
        .args(["--max-attempts", "1"])
        .output()
        .unwrap();
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    (
        output.status.code().unwrap(),
        serde_json::from_str(&text).unwrap(),
        text,
    )
}
#[test]
fn semantic_generated_input_drift_refuses_before_harness_side_effects() {
    let target = snapshot();
    let root = target.path();
    let path = input(root, json!(["load unknown"]));
    let contracts = root.join("generated/semantic-contracts.rs");
    let original = fs::read(&contracts).unwrap();
    let mut altered = original.clone();
    altered.extend(b"\n// independent neutral contract drift\n");
    fs::write(&contracts, altered).unwrap();
    let (code, value, text) = run(root, &path, "semantic-contract-drift");
    assert_eq!(code, 2);
    assert_eq!(
        value["error"]["reason"],
        "compiled_semantic_contract_binding_mismatch"
    );
    assert!(!root.join("var/semantic-contract-drift").exists());
    assert!(!text.contains("independent neutral contract drift"));
    fs::write(contracts, original).unwrap();
    let (code, value, _) = run(root, &path, "restored");
    assert_eq!(code, 1);
    assert_eq!(value["status"], "unavailable");
    assert_eq!(
        value["execution_binding"]["normalizer"]["status"],
        "matched"
    );
}

#[test]
fn harness_entry_requires_current_build_and_preserves_safe_budget_diagnostics() {
    let target = snapshot();
    let root = target.path();
    let path = input(root, json!(["load unknown"]));
    let (code, value, _) = run(root, &path, "valid");
    assert_eq!(code, 1);
    assert_eq!(value["status"], "unavailable");
    assert_eq!(
        value["execution_binding"]["normalizer"]["status"],
        "matched"
    );
    assert_eq!(
        value["execution_binding"]["executable_sha256"],
        sha256(fs::read(env!("CARGO_BIN_EXE_oev2")).unwrap())
    );
    let changed = root.join("crates/oe-engineering/src/harness/context.rs");
    let original = fs::read(&changed).unwrap();
    let mut altered = original.clone();
    altered.extend(b"\n// changed private candidate\n");
    fs::write(&changed, altered).unwrap();
    let (code, value, text) = run(root, &path, "source-drift");
    assert_eq!(code, 2);
    assert_eq!(value["error"]["reason"], "compiled_source_binding_mismatch");
    assert!(!root.join("var/source-drift").exists());
    assert!(!text.contains("private candidate"));
    fs::write(&changed, original).unwrap();
    let contracts = root.join("generated/contracts.rs");
    let original = fs::read(&contracts).unwrap();
    let mut altered = original.clone();
    altered.extend(b"\n// modified output\n");
    fs::write(&contracts, altered).unwrap();
    let (code, value, _) = run(root, &path, "contract-drift");
    assert_eq!(code, 2);
    assert_eq!(
        value["error"]["reason"],
        "compiled_generated_contract_binding_mismatch"
    );
    assert!(!root.join("var/contract-drift").exists());
    fs::write(contracts, original).unwrap();
    let path = input(root, json!(["PRIVATE_PAYLOAD_TEXT".repeat(12_000)]));
    let (code, value, text) = run(root, &path, "budget-failure");
    assert_eq!(code, 2);
    assert_eq!(
        value["error"]["code"],
        "context_requires_explicit_reselection"
    );
    assert!(value["error"]["payload_bytes"].as_u64().unwrap() > 96_000);
    assert_eq!(value["error"]["budget_bytes"], 96_000);
    assert!(!text.contains("PRIVATE_PAYLOAD_TEXT"));
    let failed: Value =
        serde_json::from_slice(&fs::read(root.join("var/budget-failure/failure.json")).unwrap())
            .unwrap();
    assert_eq!(failed["failed_stage"], "routing_context_preparation");
    assert_eq!(
        failed["error"]["payload_sha256"],
        value["error"]["payload_sha256"]
    );
    assert!(root.join("var/budget-failure/feedback.json").exists());
    assert!(!root.join("var/budget-failure/iteration.json").exists());
}
