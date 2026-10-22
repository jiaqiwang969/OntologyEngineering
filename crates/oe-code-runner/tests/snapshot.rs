//! Actual runner processes consume only inputs. Expectations were frozen by
//! the unchanged original SemanticRuntime constructor before candidate edits.
use anyhow::{Result, ensure};
use oe_core::sha256;
use oe_wire::json::{canonical_json, strict_json};
use serde_json::{Value, json};
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

const FIXED: &str = include_str!("fixtures/original-snapshot12.json");
static SERIAL: AtomicU64 = AtomicU64::new(0);
struct InputFile(PathBuf);
impl Drop for InputFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
fn invocation(capability: &str, input: Value) -> Result<Value> {
    Ok(json!({
        "schema":"oev2.code-replay-invocation/1", "case_id":"snapshot-fixed",
        "capability":capability, "profile":"local-semantic-replay/1",
        "input_sha256":sha256(canonical_json(&input)?), "input":input,
        "conditions_sha256":"d".repeat(64),
        "budget":{"timeout_ms":60000,"output_bytes":1048576},
        "seed":0,"run_id":0
    }))
}
fn run(invocation: &Value) -> Result<std::process::Output> {
    let root = std::env::temp_dir().canonicalize()?;
    let serial = SERIAL.fetch_add(1, Ordering::Relaxed);
    let path = root.join(format!(
        "oe-code-snapshot-{}-{serial}.json",
        std::process::id()
    ));
    let mut owned = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    let file = InputFile(path);
    owned.write_all(&canonical_json(invocation)?)?;
    drop(owned);
    Ok(Command::new(env!("CARGO_BIN_EXE_oe-code-runner"))
        .arg("run")
        .arg(&file.0)
        .output()?)
}
fn observation(capability: &str, input: Value) -> Result<Value> {
    let invocation = invocation(capability, input)?;
    let output = run(&invocation)?;
    ensure!(
        output.status.success(),
        "actual runner exit: {:?}, stderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let result = strict_json(&output.stdout)?;
    assert_eq!(result["input_sha256"], invocation["input_sha256"]);
    assert_eq!(result["conditions_sha256"], invocation["conditions_sha256"]);
    assert_eq!(
        result["unknown"],
        json!(["engineering_effectiveness_not_evaluated"])
    );
    assert_eq!(result["schema"], "oev2.code-runner-observation/1");
    let dto: oe_contracts::CodeRunnerObservation = serde_json::from_value(result.clone())?;
    dto.validate()
        .map_err(|e| anyhow::anyhow!("observation validation: {e:?}"))?;
    Ok(result)
}
fn input() -> Value {
    json!({"data":"<urn:s> <urn:p> \"value\" .\n", "format":"ntriples", "graph_name":null,"created_at":"2026-08-19T00:00:00Z"})
}
fn failure(input: Value, error_fragment: &str) -> Result<()> {
    let result = observation("rdf.snapshot-rdflib/1", input)?;
    assert_eq!(result["execution"], "failed");
    assert!(result["result"].is_null());
    assert_eq!(result["coverage"], json!([]));
    assert!(
        result["error"].as_str().unwrap().contains(error_fragment),
        "{result}"
    );
    Ok(())
}

#[test]
fn original_twelve_snapshots_match() -> Result<()> {
    let fixture = strict_json(FIXED.as_bytes())?;
    assert_eq!(
        fixture["original_oracle_sha256"],
        "0d98b8b9c193f8c740d6c87e5b60b46774a41103231c40043507d21adec84bf8"
    );
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 12);
    for case in cases {
        let actual = observation("rdf.snapshot-rdflib/1", case["input"].clone())?;
        assert_eq!(
            actual["execution"], "completed",
            "{}: {actual}",
            case["case_id"]
        );
        assert_eq!(
            actual["result"], case["expected_snapshot"],
            "{}",
            case["case_id"]
        );
        assert_eq!(actual["coverage"], json!(["rdf.snapshot-rdflib/1"]));
        assert!(actual["error"].is_null());
        let snapshot = &actual["result"];
        assert_eq!(
            snapshot["dataset_sha256"],
            sha256(snapshot["canonical_nquads"].as_str().unwrap().as_bytes())
        );
        assert!(
            !snapshot["canonical_nquads"]
                .as_str()
                .unwrap()
                .ends_with('\n')
        );
    }
    Ok(())
}
#[test]
fn closed_snapshot_fields_reject_caller_expectations() -> Result<()> {
    let mut value = input();
    value["expected_snapshot"] = json!({"passed":true});
    failure(value, "runner_input_unknown_or_missing_fields")
}
#[test]
fn snapshot_requires_explicit_nullable_graph_field() -> Result<()> {
    let mut value = input();
    value.as_object_mut().unwrap().remove("graph_name");
    failure(value, "runner_input_unknown_or_missing_fields")
}
#[test]
fn snapshot_field_types_remain_strict() -> Result<()> {
    for name in ["data", "format", "graph_name", "created_at"] {
        let mut value = input();
        value[name] = json!({"$serde_json::private::Number":"42"});
        failure(value, "invalid type")?;
    }
    Ok(())
}
#[test]
fn snapshot_does_not_guess_turtle_or_apply_rdfc_identity() -> Result<()> {
    for format in ["turtle", "rdfxml", "NQuads", "nt", ""] {
        let mut value = input();
        value["format"] = json!(format);
        failure(value, "runner_snapshot_input_invalid")?;
    }
    Ok(())
}
#[test]
fn malformed_rdf_is_failed_with_empty_coverage() -> Result<()> {
    let mut value = input();
    value["data"] = json!("<urn:s> <urn:p>");
    let result = observation("rdf.snapshot-rdflib/1", value)?;
    assert_eq!(result["execution"], "failed");
    assert_eq!(result["coverage"], json!([]));
    assert!(result["result"].is_null());
    assert!(result["error"].is_string());
    Ok(())
}
#[test]
fn nq_graph_override_rejects_original_dataset_ambiguity() -> Result<()> {
    let mut value = input();
    value["format"] = json!("nquads");
    value["graph_name"] = json!("urn:override");
    failure(value, "dataset_graph_override_for_dataset_syntax")
}
#[test]
fn invalid_graph_name_cannot_become_an_authority_or_relative_iri() -> Result<()> {
    let mut value = input();
    value["graph_name"] = json!("relative/graph");
    let result = observation("rdf.snapshot-rdflib/1", value)?;
    assert_eq!(result["execution"], "failed");
    assert_eq!(result["coverage"], json!([]));
    assert!(result["result"].is_null());
    Ok(())
}
#[test]
fn unknown_capability_retains_existing_unsupported_protocol() -> Result<()> {
    let result = observation("rdf.snapshot-rdflib/99", input())?;
    assert_eq!(result["execution"], "unsupported");
    assert_eq!(result["coverage"], json!([]));
    assert_eq!(
        result["error"],
        "capability_not_implemented_in_this_runner_profile"
    );
    Ok(())
}
#[test]
fn original_invocation_hash_and_frame_limits_still_reject() -> Result<()> {
    let mut value = invocation("rdf.snapshot-rdflib/1", input())?;
    value["input_sha256"] = json!("e".repeat(64));
    let output = run(&value)?;
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("runner_input_identity_changed"));
    let mut too_large = input();
    too_large["data"] = json!("x".repeat(1024 * 1024));
    let output = run(&invocation("rdf.snapshot-rdflib/1", too_large)?)?;
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    Ok(())
}
