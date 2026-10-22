use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};

#[test]
fn snapshot_checksum_cli_original_and_refusal() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let dir = oe_core::SkillRoot::new(root)
        .unwrap()
        .new_private_dir(Path::new(&format!(
            "var/maintenance/checksum-cli-{}",
            std::process::id()
        )))
        .unwrap();
    let mut fixture: Value = serde_json::from_str(include_str!(
        "../../semantica-runtime/tests/fixtures/snapshot-checksum-original141.json"
    ))
    .unwrap();
    let frozen: Value = serde_json::from_str(include_str!(
        "../../semantica-runtime/tests/fixtures/snapshot-checksum-resource-original.json"
    ))
    .unwrap();
    let digits = frozen["integer_digits"].as_u64().unwrap() as usize;
    for case in frozen["cases"].as_array().unwrap() {
        fixture["cases"].as_array_mut().unwrap().push(json!({
            "input":{
                "id":case["id"],"operation":case["operation"],
                "value":{"schema":"semantica.native-value/1","nodes":[
                    {"kind":"text","codepoints":[99,104,101,99,107,115,117,109]},
                    {"kind":"bool","value":false},
                    {"kind":"text","codepoints":[97]},
                    {"kind":"int","decimal":format!("1{}", "0".repeat(digits-1))},
                    {"kind":"dict","entries":[[0,1],[2,3]]}
                ],"root":4}
            },"expected":case["expected"]
        }));
    }
    for (i, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let c = &case["input"];
        let request = json!({"schema":"semantica.snapshot-checksum-request/1","operation":c["operation"],"representation":if c["value"]["schema"]=="semantica.native-value/1"{"native_value"}else{"plain_json"},"value":c["value"],"options":{}});
        let input = dir.join(format!("{i}-input.json"));
        let output = dir.join(format!("{i}-output.json"));
        fs::write(&input, serde_json::to_vec(&request).unwrap()).unwrap();
        let r = Command::new(env!("CARGO_BIN_EXE_oev2"))
            .env_clear()
            .env("PATH", "")
            .arg("--root")
            .arg(root)
            .args(["kernel", "snapshot-checksum", "--input"])
            .arg(&input)
            .arg("--output")
            .arg(&output)
            .output()
            .unwrap();
        assert!(
            r.status.success(),
            "{}:{}",
            c["id"],
            String::from_utf8_lossy(&r.stderr)
        );
        assert_eq!(r.stdout, fs::read(&output).unwrap());
        let got: Value = serde_json::from_slice(&r.stdout).unwrap();
        assert_eq!(got["result"], case["expected"]["result"], "{}", c["id"]);
        if !case["expected"]["error"].is_null() {
            let e = &case["expected"]["error"];
            assert_eq!(
                got["failure"]["message"],
                format!(
                    "{}: {}",
                    e["type"].as_str().unwrap(),
                    e["message"].as_str().unwrap()
                )
            );
        } else {
            assert!(got["failure"].is_null());
        }
    }
    let input = dir.join("unknown-operation.json");
    let output = dir.join("refused-output.json");
    fs::write(&input,serde_json::to_vec(&json!({"schema":"semantica.snapshot-checksum-request/1","operation":"adopt","representation":"plain_json","value":{},"options":{}})).unwrap()).unwrap();
    let r = Command::new(env!("CARGO_BIN_EXE_oev2"))
        .arg("--root")
        .arg(root)
        .args(["kernel", "snapshot-checksum", "--input"])
        .arg(input)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert_eq!(r.status.code(), Some(2));
    assert!(r.stdout.is_empty() && !output.exists());
}
