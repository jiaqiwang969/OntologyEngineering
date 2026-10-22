//! Current executable, real CPU weights and synthetic owner pool; no live key/API.
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}
fn gold() -> Value {
    serde_json::from_slice(include_bytes!(
        "../../semantica-runtime/tests/fixtures/native-cpu-oracles.json"
    ))
    .unwrap()
}
fn request() -> Value {
    json!({"schema":"semantica.native-request/1","model":gold()["model_specs"]["bge-small"],
    "operation":"embed","interface_profile":"text_embedder","batch":true,
    "texts":gold()["models"]["bge-small"]["texts"],"normalize":true,"options":{},"seed":77})
}
fn invoke(input: Value, max: u64, empty_source: bool) -> (std::process::Output, Value) {
    let root = root();
    fs::create_dir_all(root.join("var")).unwrap();
    let case = tempfile::tempdir_in(root.join("var")).unwrap();
    let key = case.path().join("synthetic-key");
    fs::write(&key, &["sk-", "SYNTHETIC_NATIVE_CLI_ONLY"].concat()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&key, fs::Permissions::from_mode(0o600)).unwrap();
    }
    let file = case.path().join("request.json");
    fs::write(&file, serde_json::to_vec(&input).unwrap()).unwrap();
    let source = if empty_source {
        let p = case.path().join("empty-source");
        fs::create_dir(&p).unwrap();
        p
    } else {
        std::env::var_os("SEMANTICA_NATIVE_FIXTURE_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("sources/native-models"))
    };
    let output = Command::new(env!("CARGO_BIN_EXE_oev2"))
        .arg("--root")
        .arg(&root)
        .args(["providers", "native", "--input"])
        .arg(file)
        .arg("--model-source-root")
        .arg(source)
        .arg("--output")
        .arg(case.path().join("result"))
        .arg("--credential-file")
        .arg(key)
        .arg("--max-calls")
        .arg(max.to_string())
        .env("PATH", "")
        .output()
        .unwrap();
    assert!(!String::from_utf8_lossy(&output.stdout).contains("SYNTHETIC_NATIVE_CLI_ONLY"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("SYNTHETIC_NATIVE_CLI_ONLY"));
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    (output, value)
}
#[test]
#[ignore = "requires registered exact public model bytes; explicitly run for native acceptance"]
fn actual_native_cli_executes_bound_real_bert_without_python_or_network() {
    let (output, value) = invoke(request(), 1, false);
    assert_eq!(output.status.code(), Some(0), "{value}");
    assert_eq!(value["execution_kind"], "native_cpu");
    assert_eq!(value["network_dispatches"], 0);
    assert_eq!(value["python_production_delegate"], false);
    assert_eq!(
        value["response"]["result"]["output"]["shape"],
        json!([4, 384])
    );
    assert_eq!(value["calls"]["attempts"], 1);
    assert_eq!(value["calls"]["unreported_attempts"], 1);
    assert_eq!(value["cost"]["status"], "unknown");
    assert_eq!(value["fact_admission"], false);
    assert_eq!(value["engineering_acceptance"], "not_determined");
    assert_eq!(
        value["execution_binding"]["normalizer"]["status"],
        "matched"
    );
    assert!(
        value["provenance_snapshot"]["canonical_nquads"]
            .as_str()
            .unwrap()
            .contains("native_model_inference")
    );
    let bits = value["response"]["result"]["output"]["bits"]
        .as_array()
        .unwrap();
    let golden = gold();
    let expected = golden["models"]["bge-small"]["sentence_embeddings_normalized"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|row| row.as_array().unwrap().iter().map(|v| v.as_f64().unwrap()))
        .collect::<Vec<_>>();
    assert_eq!(bits.len(), expected.len());
    let max = bits
        .iter()
        .zip(expected)
        .map(|(v, g)| (f32::from_bits(v.as_u64().unwrap() as u32) as f64 - g).abs())
        .fold(0.0, f64::max);
    assert!(
        max <= golden["tolerance"]["bert_embedding_abs"].as_f64().unwrap(),
        "original frozen weighted tolerance: {max}"
    );
}
#[test]
fn native_cli_zero_budget_never_reads_missing_model_assets() {
    let (output, value) = invoke(request(), 0, true);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        value["response"]["failure"]["code"],
        "shared_model_call_budget_exhausted"
    );
    assert_eq!(value["calls"]["attempts"], 0);
    assert_eq!(value["provenance_snapshot"]["quad_count"], 0);
}
#[test]
fn native_cli_source_failure_consumes_one_reservation_without_fabricating_result() {
    let (output, value) = invoke(request(), 1, true);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        value["response"]["failure"]["code"],
        "native_asset_unavailable"
    );
    assert_eq!(value["calls"]["attempts"], 1);
    assert_eq!(value["calls"]["unreported_attempts"], 1);
    assert!(value["response"]["result"].is_null());
    assert_eq!(value["provenance_snapshot"]["quad_count"], 0);
}
