use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};
#[test]
fn temporal_patterns_cli_executes_and_retains_state() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let dir = oe_core::SkillRoot::new(root)
        .unwrap()
        .new_private_dir(Path::new(&format!(
            "var/maintenance/temporal-patterns-cli-tests-{}",
            std::process::id()
        )))
        .unwrap();
    let histories: Vec<Value> = serde_json::from_str(include_str!(
        "../../semantica-runtime/tests/fixtures/temporal-patterns-original79.json"
    ))
    .unwrap();
    for h in histories {
        let id = h["id"].as_str().unwrap();
        let input = dir.join(format!("{id}-input.json"));
        let output = dir.join(format!("{id}-result.json"));
        fs::write(&input, serde_json::to_vec(&h["request"]).unwrap()).unwrap();
        let got = Command::new(env!("CARGO_BIN_EXE_oev2"))
            .env_clear()
            .env("PATH", "")
            .arg("--root")
            .arg(root)
            .args(["kernel", "temporal-patterns", "--input"])
            .arg(input)
            .arg("--output")
            .arg(&output)
            .output()
            .unwrap();
        fs::write(dir.join(format!("{id}.stdout")), &got.stdout).unwrap();
        fs::write(dir.join(format!("{id}.stderr")), &got.stderr).unwrap();
        assert!(
            got.status.success(),
            "{id}: {}",
            String::from_utf8_lossy(&got.stderr)
        );
        let out: Value = serde_json::from_slice(&got.stdout).unwrap();
        assert_eq!(
            out,
            serde_json::from_slice::<Value>(&fs::read(output).unwrap()).unwrap()
        );
        assert_eq!(out["final_config"], h["final_config"]);
        for (s, e) in out["steps"]
            .as_array()
            .unwrap()
            .iter()
            .zip(h["steps"].as_array().unwrap())
        {
            for k in [
                "result",
                "config_after",
                "occurrence_relationship_indices",
                "clock_fallback_count",
            ] {
                assert_eq!(s[k], e[k], "{id}:{k}");
            }
            assert_eq!(s["failure"].is_null(), e["source_error"].is_null());
        }
    }
    let input = dir.join("invalid-input.json");
    let output = dir.join("invalid-output.json");
    fs::write(
        &input,
        serde_json::to_vec(
            &json!({"schema":"bad","clock_at":"2024-01-01","initial_config":{},"actions":[]}),
        )
        .unwrap(),
    )
    .unwrap();
    let got = Command::new(env!("CARGO_BIN_EXE_oev2"))
        .env_clear()
        .env("PATH", "")
        .arg("--root")
        .arg(root)
        .args(["kernel", "temporal-patterns", "--input"])
        .arg(input)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert_eq!(got.status.code(), Some(2));
    assert!(!output.exists());
    assert!(got.stdout.is_empty());
    let e: Value = serde_json::from_slice(&got.stderr).unwrap();
    assert_eq!(e["execution"], "failed");
    assert_eq!(e["conclusion"], "unknown");
}
