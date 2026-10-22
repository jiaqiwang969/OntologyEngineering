use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};
#[test]
fn temporal_version_diff_cli_source_and_refusal() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let dir = oe_core::SkillRoot::new(root)
        .unwrap()
        .new_private_dir(Path::new(&format!(
            "var/maintenance/temporal-diff-cli-{}",
            std::process::id()
        )))
        .unwrap();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../semantica-runtime/tests/fixtures/temporal-version-diff-original54.json"
    ))
    .unwrap();
    for (i, case) in fixture["inputs"]["cases"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let req = json!({"schema":"semantica.temporal-version-diff-request/1","version1":case["version1"],"version2":case["version2"],"stored_versions":case["stored_versions"],"hash_seed":0,"comparison_metrics":null,"options":{}});
        let input = dir.join(format!("{i}-input.json"));
        let output = dir.join(format!("{i}-output.json"));
        fs::write(&input, serde_json::to_vec(&req).unwrap()).unwrap();
        let r = Command::new(env!("CARGO_BIN_EXE_oev2"))
            .env_clear()
            .env("PATH", "")
            .arg("--root")
            .arg(root)
            .args(["kernel", "temporal-version-diff", "--input"])
            .arg(&input)
            .arg("--output")
            .arg(&output)
            .output()
            .unwrap();
        assert!(
            r.status.success(),
            "{}:{}",
            case["id"],
            String::from_utf8_lossy(&r.stderr)
        );
        let got: Value = serde_json::from_slice(&r.stdout).unwrap();
        let expected = &fixture["observed"][0]["cases"][i];
        assert_eq!(
            serde_json::to_string(&got["result"]).unwrap(),
            serde_json::to_string(&expected["result"]).unwrap(),
            "{}",
            case["id"]
        );
        assert_eq!(got["storage_reads"], expected["storage_reads"]);
        assert_eq!(r.stdout, fs::read(&output).unwrap());
        if !expected["error"].is_null() {
            assert_eq!(
                got["failure"]["message"],
                format!(
                    "{}: {}",
                    expected["error"]["type"].as_str().unwrap(),
                    expected["error"]["message"].as_str().unwrap()
                )
            );
        }
    }
    for (name, seed) in [("invalid-seed", 4294967296u64), ("invalid-schema", 0)] {
        let req = json!({"schema":if seed==0{"invalid"}else{"semantica.temporal-version-diff-request/1"},"version1":{},"version2":{},"stored_versions":{},"hash_seed":seed,"comparison_metrics":null,"options":{}});
        let input = dir.join(format!("{name}-input.json"));
        let output = dir.join(format!("{name}-output.json"));
        fs::write(&input, serde_json::to_vec(&req).unwrap()).unwrap();
        let r = Command::new(env!("CARGO_BIN_EXE_oev2"))
            .arg("--root")
            .arg(root)
            .args(["kernel", "temporal-version-diff", "--input"])
            .arg(input)
            .arg("--output")
            .arg(&output)
            .output()
            .unwrap();
        assert_eq!(r.status.code(), Some(2));
        assert!(r.stdout.is_empty() && !output.exists());
    }
}
