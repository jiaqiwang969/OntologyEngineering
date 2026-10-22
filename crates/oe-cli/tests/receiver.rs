use oe_core::{SkillRoot, sha256};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};
static SERIAL: AtomicU64 = AtomicU64::new(0);

fn fixture() -> PathBuf {
    let original = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let root = SkillRoot::new(&original)
        .unwrap()
        .new_private_dir(Path::new(&format!(
            "var/maintenance/receiver-tests/{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        )))
        .unwrap();
    for p in [
        "SKILL.md",
        "AGENTS.md",
        "README.md",
        "VERSION",
        "LICENSE",
        "crates/example/src/backend.rs",
        "crates/example/src/backend/dom.rs",
    ] {
        let path = root.join(p);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, format!("synthetic receiver fixture: {p}\n")).unwrap();
    }
    for p in ["var/private-record.json", "sources/original.txt"] {
        let path = root.join(p);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "PRIVATE_SYNTHETIC_MARKER").unwrap();
    }
    // The manifest enumerates runtime/vendor; an empty directory is a valid input here.
    fs::create_dir_all(root.join("runtime/vendor")).unwrap();
    root
}
fn run(root: &Path, label: &str, arguments: &[&str]) -> Output {
    let result = Command::new(env!("CARGO_BIN_EXE_oev2"))
        .env_clear()
        .env("PATH", "")
        .arg("--root")
        .arg(root)
        .args(arguments)
        .output()
        .unwrap();
    fs::write(root.join(format!("var/{label}.stdout")), &result.stdout).unwrap();
    fs::write(root.join(format!("var/{label}.stderr")), &result.stderr).unwrap();
    result
}
fn manifest(root: &Path) -> Value {
    let result = run(
        root,
        "manifest",
        &["maintain", "manifest", "--output", "var/inventory.json"],
    );
    assert!(
        result.status.success(),
        "actual manifest failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&fs::read(root.join("var/inventory.json")).unwrap()).unwrap()
}
#[test]
fn nested_path_components_have_portable_manifest_string_order() {
    let root = fixture();
    let inventory = manifest(&root);
    let paths: Vec<_> = inventory["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["path"].as_str().unwrap())
        .collect();
    assert!(paths.windows(2).all(|w| w[0] < w[1]));
    assert!(
        !paths
            .iter()
            .any(|p| p.starts_with("var/") || p.starts_with("sources/"))
    );
    assert!(
        !String::from_utf8(serde_json::to_vec(&inventory).unwrap())
            .unwrap()
            .contains("PRIVATE_SYNTHETIC_MARKER")
    );
}
#[test]
fn source_drift_is_rejected_before_receiver_publication() {
    let root = fixture();
    let mut inventory = manifest(&root);
    inventory["files"][0]["sha256"] = Value::String("0".repeat(64));
    fs::write(
        root.join("var/changed-inventory.json"),
        serde_json::to_vec(&inventory).unwrap(),
    )
    .unwrap();
    let result = run(
        &root,
        "rejected",
        &[
            "maintain",
            "install",
            "--inventory",
            "var/changed-inventory.json",
            "--target",
            "var/receiver",
        ],
    );
    assert!(!result.status.success());
    assert!(!root.join("var/receiver").exists());
    assert!(!fs::read_dir(root.join("var")).unwrap().any(|e| {
        e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".oev2-receiving-")
    }));
    let failure: Value = serde_json::from_slice(&result.stderr).unwrap();
    assert_eq!(failure["error"]["reason"], "receiver_source_drift");
    assert_eq!(failure["conclusion"], "unknown");
}
#[test]
fn actual_receiving_checks_all_bytes_and_never_overwrites_an_existing_target() {
    let root = fixture();
    let inventory = manifest(&root);
    let args = [
        "maintain",
        "install",
        "--inventory",
        "var/inventory.json",
        "--target",
        "var/receiver",
    ];
    let result = run(&root, "received", &args);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["status"], "source_received");
    assert_eq!(report["build"], "not_run");
    let received = root.join("var/receiver");
    for row in inventory["files"].as_array().unwrap() {
        let bytes = fs::read(received.join(row["path"].as_str().unwrap())).unwrap();
        assert_eq!(sha256(&bytes), row["sha256"].as_str().unwrap());
        assert_eq!(bytes.len() as u64, row["bytes"].as_u64().unwrap());
    }
    let previous = fs::read(received.join("SKILL.md")).unwrap();
    let rejected = run(&root, "existing", &args);
    assert!(!rejected.status.success());
    assert_eq!(fs::read(received.join("SKILL.md")).unwrap(), previous);
    assert!(!received.join("sources").exists());
    assert!(!received.join("var").exists());
}

#[test]
fn v2_failures_preserve_known_reason_without_echoing_private_inputs() {
    let root = fixture();
    let marker = "PRIVATE_SYNTHETIC_MARKER";
    let input = serde_json::json!({"schema":"oev2.project/1","project_id":marker,"revision":"","purpose":marker,"objects":{},"conditions":{},"evidence":{},"decisions":[],"dependencies":{}});
    fs::write(
        root.join("var/private-input.json"),
        serde_json::to_vec(&input).unwrap(),
    )
    .unwrap();
    let result = run(
        &root,
        "invalid-project",
        &["project", "validate", "--input", "var/private-input.json"],
    );
    assert!(!result.status.success());
    let response: Value = serde_json::from_slice(&result.stderr).unwrap();
    assert_eq!(response["error"]["reason"], "project_identity_invalid");
    assert_eq!(response["conclusion"], "unknown");
    for bytes in [&result.stdout, &result.stderr] {
        assert!(!String::from_utf8_lossy(bytes).contains(marker));
    }
    let result = run(&root, "invalid-arguments", &["project", "validate", marker]);
    assert!(!result.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&result.stdout).unwrap()["error"]["reason"],
        "invalid_arguments"
    );
    for bytes in [&result.stdout, &result.stderr] {
        assert!(!String::from_utf8_lossy(bytes).contains(marker));
    }
}

#[test]
fn a_declared_v1_root_cannot_be_used_as_the_v2_working_root() {
    let root = fixture();
    fs::write(
        root.join("SKILL.md"),
        "---\nname: ontology-engineering\ndescription: synthetic historical root\n---\n",
    )
    .unwrap();
    fs::write(root.join("runtime/semantic-bundles.json"), "{}").unwrap();
    fs::create_dir_all(root.join("var/maintenance")).unwrap();
    let old_status = br#"{"status":"OLD_ROOT_MARKER"}"#;
    fs::write(root.join("var/maintenance/status.json"), old_status).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_oev2"))
        .current_dir(&root)
        .env_clear()
        .env("PATH", "")
        .args(["maintain", "status"])
        .output()
        .unwrap();
    fs::write(root.join("var/default-root.stdout"), &result.stdout).unwrap();
    fs::write(root.join("var/default-root.stderr"), &result.stderr).unwrap();
    assert!(
        !String::from_utf8_lossy(&result.stdout).contains("OLD_ROOT_MARKER"),
        "v1 was selected as default root"
    );
    let result = run(&root, "explicit-v1-root", &["maintain", "status"]);
    assert!(!result.status.success(), "explicit v1 root was accepted");
    assert_eq!(
        serde_json::from_slice::<Value>(&result.stderr).unwrap()["error"]["reason"],
        "legacy_skill_root_read_only"
    );
    assert_eq!(
        fs::read(root.join("var/maintenance/status.json")).unwrap(),
        old_status
    );
}
