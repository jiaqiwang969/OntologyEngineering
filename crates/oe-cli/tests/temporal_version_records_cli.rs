use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};
#[test]
fn temporal_version_records_cli_original_and_rejection() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let dir = oe_core::SkillRoot::new(root)
        .unwrap()
        .new_private_dir(Path::new(&format!(
            "var/maintenance/temporal-version-records-cli-tests-{}",
            std::process::id()
        )))
        .unwrap();
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "../../semantica-runtime/tests/fixtures/temporal-version-records-original102.json"
    ))
    .unwrap();
    for case in &cases {
        let id = case["id"].as_str().unwrap();
        let input = dir.join(format!("{id}-input.json"));
        let output = dir.join(format!("{id}-result.json"));
        fs::write(&input, serde_json::to_vec(&case["request"]).unwrap()).unwrap();
        let got = Command::new(env!("CARGO_BIN_EXE_oev2"))
            .env_clear()
            .env("PATH", "")
            .arg("--root")
            .arg(root)
            .args(["kernel", "temporal-version-records", "--input"])
            .arg(input)
            .arg("--output")
            .arg(&output)
            .output()
            .unwrap();
        fs::write(dir.join(format!("{id}.stdout")), &got.stdout).unwrap();
        fs::write(dir.join(format!("{id}.stderr")), &got.stderr).unwrap();
        assert!(
            got.status.success(),
            "{id}:{}",
            String::from_utf8_lossy(&got.stderr)
        );
        let out: Value = serde_json::from_slice(&got.stdout).unwrap();
        assert_eq!(
            out,
            serde_json::from_slice::<Value>(&fs::read(&output).unwrap()).unwrap()
        );
        for key in ["result", "clock_fallback_count"] {
            assert_eq!(out[key], case["expected"][key], "{id}:{key}");
        }
        let error = &case["expected"]["source_error"];
        if error.is_null() {
            assert!(out["failure"].is_null());
        } else {
            assert_eq!(
                out["failure"]["message"],
                format!(
                    "{}: {}",
                    error["type"].as_str().unwrap(),
                    error["message"].as_str().unwrap()
                )
            );
        }
    }
    let numeric: Vec<Value> = serde_json::from_str(include_str!(
        "../../semantica-runtime/tests/fixtures/temporal-version-records-integer-boundary.json"
    ))
    .unwrap();
    for case in numeric {
        let id = case["id"].as_str().unwrap();
        let input = dir.join(format!("numeric-{id}-input.json"));
        let output = dir.join(format!("numeric-{id}-result.json"));
        fs::write(&input, case["request_raw"].as_str().unwrap()).unwrap();
        let got = Command::new(env!("CARGO_BIN_EXE_oev2"))
            .env_clear()
            .env("PATH", "")
            .arg("--root")
            .arg(root)
            .args(["kernel", "temporal-version-records", "--input"])
            .arg(input)
            .arg("--output")
            .arg(&output)
            .output()
            .unwrap();
        if case["expected_admitted"] == true {
            assert!(got.status.success(), "{id}");
            let out: Value = serde_json::from_slice(&got.stdout).unwrap();
            assert_eq!(out["result"], case["expected_result"]);
            assert_eq!(
                out,
                serde_json::from_slice::<Value>(&fs::read(output).unwrap()).unwrap()
            );
        } else {
            assert_eq!(got.status.code(), Some(2), "{id}");
            assert!(got.stdout.is_empty() && !output.exists());
            let failure: Value = serde_json::from_slice(&got.stderr).unwrap();
            assert_eq!(failure["execution"], "failed");
            assert_eq!(failure["conclusion"], "unknown");
        }
    }
    for id in [
        "invalid-schema",
        "wire-overflow",
        "preserve-existing-output",
    ] {
        let input = dir.join(format!("{id}-input.json"));
        let output = dir.join(format!("{id}-result.json"));
        let mut request = cases[0]["request"].clone();
        if id == "invalid-schema" {
            request["schema"] = json!("bad");
        }
        if id == "wire-overflow" {
            request["arguments"]["unused"] = json!("x".repeat(1024 * 1024));
        }
        if id == "preserve-existing-output" {
            fs::write(&output, b"original evidence").unwrap();
        }
        fs::write(&input, serde_json::to_vec(&request).unwrap()).unwrap();
        let got = Command::new(env!("CARGO_BIN_EXE_oev2"))
            .env_clear()
            .env("PATH", "")
            .arg("--root")
            .arg(root)
            .args(["kernel", "temporal-version-records", "--input"])
            .arg(input)
            .arg("--output")
            .arg(&output)
            .output()
            .unwrap();
        assert_eq!(got.status.code(), Some(2), "{id}");
        assert!(got.stdout.is_empty());
        if id == "preserve-existing-output" {
            assert_eq!(fs::read(output).unwrap(), b"original evidence");
        } else {
            assert!(!output.exists());
        }
        let e: Value = serde_json::from_slice(&got.stderr).unwrap();
        assert_eq!(e["execution"], "failed");
        assert_eq!(e["conclusion"], "unknown");
    }
}
