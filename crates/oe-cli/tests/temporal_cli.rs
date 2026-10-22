use serde_json::Value;
use std::{fs, path::Path, process::Command};

#[test]
fn temporal_cli_executes_without_python_and_retains_failures() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let dir = oe_core::SkillRoot::new(root)
        .unwrap()
        .new_private_dir(Path::new(&format!(
            "var/maintenance/temporal-cli-tests-{}",
            std::process::id()
        )))
        .unwrap();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../semantica-runtime/tests/fixtures/temporal-interval-original66.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        let input = dir.join(format!("{id}-input.json"));
        let output = dir.join(format!("{id}-result.json"));
        fs::write(&input, serde_json::to_vec(&case["request"]).unwrap()).unwrap();
        let got = Command::new(env!("CARGO_BIN_EXE_oev2"))
            .env_clear()
            .env("PATH", "")
            .arg("--root")
            .arg(root)
            .args(["kernel", "temporal", "--input"])
            .arg(input)
            .arg("--output")
            .arg(&output)
            .output()
            .unwrap();
        fs::write(dir.join(format!("{id}.stdout")), &got.stdout).unwrap();
        fs::write(dir.join(format!("{id}.stderr")), &got.stderr).unwrap();
        if case["expected_error"] == true {
            assert_eq!(got.status.code(), Some(2), "{id}");
            assert!(!output.exists());
            continue;
        }
        assert!(
            got.status.success(),
            "{id}: {} {}",
            String::from_utf8_lossy(&got.stdout),
            String::from_utf8_lossy(&got.stderr)
        );
        let result: Value = serde_json::from_slice(&got.stdout).unwrap();
        assert_eq!(
            result,
            serde_json::from_slice::<Value>(&fs::read(output).unwrap()).unwrap()
        );
        for (key, value) in case["expected"].as_object().unwrap() {
            assert_eq!(&result[key], value, "{id}:{key}");
        }
    }
}
