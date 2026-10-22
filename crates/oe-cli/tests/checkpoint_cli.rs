//! Actual process check: explicit local recording must not need credentials.
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};

#[test]
fn checkpoint_process_records_without_path_or_credentials_and_preserves_collision() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .canonicalize()
        .unwrap();
    let test_root = root.join("var/tests/unified-system/cli");
    fs::create_dir_all(&test_root).unwrap();
    let temp = tempfile::Builder::new()
        .prefix("checkpoint-cli-")
        .tempdir_in(&test_root)
        .unwrap();
    let scratch = temp.path();
    let input = json!({"schema":"oev2.harness-checkpoint-input/1", "task_id":"no-credentials",
        "context":"Record a failed native attempt and an unresolved load.", "request":"Keep original observations for subsequent engineering work.",
        "harness":{"project_id":"independent", "decision_id":"support", "object_id":"part", "revision":"r1",
            "problem_class":"engineering-support", "goal":"Preserve conditions before another attempt", "conditions":[],
            "constraints":["No physical adoption without load evidence"], "unknowns":["load"],
            "semantic_modules":[], "review_requests":[], "observations":[],
            "checkpoint_policy":{"schema":"oev2.harness-checkpoint-policy/1", "mode":"verified_no_model",
                "byte_budget":16777216, "reason":"Explicit local recording independent of model service"}}});
    fs::write(
        scratch.join("input.json"),
        serde_json::to_vec(&input).unwrap(),
    )
    .unwrap();
    let call = || {
        Command::new(env!("CARGO_BIN_EXE_oev2"))
            .env_clear()
            .env("PATH", "")
            .arg("--root")
            .arg(&root)
            .args(["harness", "step", "--input"])
            .arg(scratch.join("input.json"))
            .arg("--output")
            .arg(scratch.join("record"))
            .output()
            .unwrap()
    };
    let output = call();
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let path = scratch.join("record/iteration.json");
    let original = fs::read(&path).unwrap();
    let result: Value = serde_json::from_slice(&original).unwrap();
    assert_eq!(result["model_execution"], "not_run");
    assert_eq!(result["model_calls"], 0);
    assert_eq!(result["status"], "recorded");
    assert_eq!(result["engineering_acceptance"], "not_determined");
    assert_eq!(result["current_use"], "blocked_or_historical_only");
    assert!(!scratch.join("deliberately-absent.txt").exists());
    assert!(!call().status.success());
    assert_eq!(fs::read(path).unwrap(), original);
}
