use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}
fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_oev2"))
        .env_clear()
        .env("PATH", "")
        .arg("--root")
        .arg(root)
        .args(args)
        .output()
        .unwrap()
}
fn request(model: &str, operation: &str, input: &Value) -> Value {
    let raw = serde_json::to_string(input).unwrap();
    json!({"schema":"semantica.schema-request/1","model_id":model,"profile":"semantica.pydantic-json/1",
        "operation":operation,"input_sha256":oe_core::sha256(raw.as_bytes()),"input_json":raw})
}
fn evaluate(root: &Path, value: &Value) -> Output {
    fs::create_dir_all(root.join("var")).unwrap();
    let directory = tempfile::tempdir_in(root.join("var")).unwrap();
    let path = directory.path().join("schema-request.json");
    fs::write(&path, serde_json::to_vec(value).unwrap()).unwrap();
    run(
        root,
        &["schemas", "evaluate", "--input", path.to_str().unwrap()],
    )
}

#[test]
fn actual_cli_consumes_all_106_models_and_keeps_construct_validation_and_adoption_distinct() {
    let root = root();
    let catalog = run(&root, &["schemas", "list"]);
    assert!(
        catalog.status.success(),
        "{}",
        String::from_utf8_lossy(&catalog.stderr)
    );
    let catalog: Value = serde_json::from_slice(&catalog.stdout).unwrap();
    assert_eq!(catalog["models"].as_array().unwrap().len(), 106);
    assert_eq!(catalog["engineering_adoption"], "not_established");
    let frozen: Value = serde_json::from_slice(
        &fs::read(
            root.parent()
                .unwrap()
                .join("ORIGINAL106-FROZEN-EXPECTATIONS.json"),
        )
        .unwrap(),
    )
    .unwrap();
    for case in frozen["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["case"] == "minimal")
    {
        let output = evaluate(
            &root,
            &request(
                case["model"].as_str().unwrap(),
                "model_validate",
                &case["input"],
            ),
        );
        assert_eq!(
            output.status.code(),
            Some(0),
            "{}: {}",
            case["model"],
            String::from_utf8_lossy(&output.stderr)
        );
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["result"]["output"], case["original"]["output"]);
        assert_eq!(value["result"]["validation_executed"], true);
        assert_eq!(value["engineering_adoption"], "not_established");
        assert_eq!(value["model_execution"], "not_run");
    }
    let id = "semantica.semantic_extract.schemas::EntityOut";
    let invalid = evaluate(&root, &request(id, "model_validate", &json!({})));
    assert_eq!(invalid.status.code(), Some(2));
    let invalid: Value = serde_json::from_slice(&invalid.stdout).unwrap();
    assert_eq!(invalid["result"]["valid"], false);
    assert_eq!(invalid["result"]["issues"].as_array().unwrap().len(), 2);
    let unchecked = evaluate(&root, &request(id, "model_construct", &json!({})));
    assert_eq!(unchecked.status.code(), Some(0));
    let unchecked: Value = serde_json::from_slice(&unchecked.stdout).unwrap();
    assert_eq!(unchecked["result"]["valid"], false);
    assert_eq!(unchecked["result"]["validation_executed"], false);
    assert!(unchecked["result"]["issues"].as_array().unwrap().is_empty());
}

#[test]
fn cli_rejects_input_source_and_compiled_neutral_contract_drift() {
    let root = root();
    let mut input = request(
        "semantica.semantic_extract.schemas::EntityOut",
        "model_validate",
        &json!({"text":"A","label":"part"}),
    );
    input["input_sha256"] = json!("0".repeat(64));
    let mismatch = evaluate(&root, &input);
    assert!(!mismatch.status.success());
    assert!(String::from_utf8_lossy(&mismatch.stderr).contains("schema_input_sha_mismatch"));
    let directory = tempfile::tempdir_in(root.join("var")).unwrap();
    let target = directory.path();
    let manifest = oe_design::provenance::compiled().unwrap();
    let mut files = manifest["files"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    files.extend(
        [
            "SKILL.md",
            "generated/contracts.rs",
            "generated/semantic-contracts.rs",
        ]
        .map(str::to_owned),
    );
    for path in files {
        let destination = target.join(&path);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::copy(root.join(&path), destination).unwrap();
    }
    let contracts = target.join("generated/semantic-contracts.rs");
    let mut bytes = fs::read(&contracts).unwrap();
    bytes.extend(b"\n// independent schema receiver drift\n");
    fs::write(contracts, bytes).unwrap();
    let drift = run(target, &["schemas", "list"]);
    assert!(!drift.status.success());
    assert!(
        String::from_utf8_lossy(&drift.stderr)
            .contains("compiled_semantic_contract_binding_mismatch")
    );
}
