//! Readback of exact dispatch receipts. Neither semantic nor physical acceptance.
use crate::protocol::{
    ArtifactIdentity, JOB_SCHEMA, JobResult, JobStatus, artifacts, artifacts_sha256,
};
use anyhow::{Context, Result, ensure};
use oe_core::{FileIdentity, fingerprint_file, read_regular_bytes, sha256};
use serde_json::{Value, json};
use std::path::Path;

pub const MAX_JOB_RECORD_BYTES: usize = 1024 * 1024;
#[derive(Debug, Clone)]
pub struct AcceptanceResult {
    pub job_id: String,
    pub status: String,
    pub output_sha256: String,
    pub errors: Vec<String>,
}
impl AcceptanceResult {
    pub fn passed(&self) -> bool {
        self.status == "accepted" && self.errors.is_empty()
    }
    pub fn to_json(&self) -> Value {
        json!({"schema":"ontology-engineering.geometry-acceptance/v1","job_id":self.job_id,
            "status":self.status,"output_sha256":self.output_sha256,"errors":self.errors,
            "engineering_acceptance":"not_determined","note":"exact dispatch readback only; not a physical or semantic verdict"})
    }
}
/// Historical API cannot establish receipt identity. It always returns rejected;
/// use the actual dispatch result with `verify_job_result` for bound readback.
pub fn verify_job_output(output_dir: &Path, expected_job_id: &str) -> Result<AcceptanceResult> {
    ensure!(
        !expected_job_id.is_empty()
            && expected_job_id.len() <= 128
            && expected_job_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-'),
        "invalid_geometry_job_id"
    );
    oe_core::no_symlinks(output_dir)?;
    ensure!(output_dir.is_dir(), "geometry_output_directory_missing");
    Ok(AcceptanceResult {
        job_id: expected_job_id.to_owned(),
        status: "rejected".to_owned(),
        output_sha256: String::new(),
        errors: vec!["geometry_receipt_identity_not_bound; use verify_job_result".to_owned()],
    })
}
/// The forward execution's exact receipt is the input to subsequent readback.
pub fn verify_job_result(expected: &JobResult) -> Result<AcceptanceResult> {
    let directory = &expected.output_dir;
    oe_core::no_symlinks(directory)?;
    let name = format!("{}.job.json", expected.job_id);
    let bytes = read_regular_bytes(&directory.join(&name), MAX_JOB_RECORD_BYTES)?;
    ensure!(
        sha256(&bytes) == expected.record_sha256,
        "geometry_job_record_hash_mismatch"
    );
    let record = oe_core::json::strict_json(&bytes)?;
    ensure!(
        record["schema"] == JOB_SCHEMA
            && record["identity"]["job_id"] == expected.job_id
            && record["identity"]["input_sha256"] == expected.input_sha256,
        "geometry_job_record_identity_mismatch"
    );
    let mut errors = Vec::new();
    let status: JobStatus = serde_json::from_value(record["status"].clone())?;
    if status != JobStatus::Completed || expected.status != JobStatus::Completed {
        errors.push("geometry_job_not_completed".to_owned());
    }
    if record["exit_code"] != 0 || expected.exit_code != Some(0) {
        errors.push("geometry_process_not_successful".to_owned());
    }
    let receipt: FileIdentity = serde_json::from_value(record["process_receipt"].clone())?;
    let process_path = directory.join("_oe/process/process.json");
    ensure!(
        fingerprint_file(&process_path, MAX_JOB_RECORD_BYTES as u64)? == receipt,
        "geometry_process_receipt_hash_mismatch"
    );
    let process =
        oe_core::json::strict_json(&read_regular_bytes(&process_path, MAX_JOB_RECORD_BYTES)?)?;
    if process["status"] != "completed"
        || process["exit_code"] != 0
        || process["capture_complete"] != true
    {
        errors.push("geometry_process_capture_not_completed".to_owned());
    }
    let stdout = read_regular_bytes(
        &directory.join("_oe/process/stdout"),
        crate::protocol::MAX_OUTPUT_BYTES,
    )?;
    let stdout_hash = sha256(&stdout);
    let stderr = read_regular_bytes(
        &directory.join("_oe/process/stderr"),
        crate::protocol::MAX_OUTPUT_BYTES,
    )?;
    ensure!(
        process["stderr"]["sha256"] == sha256(&stderr),
        "geometry_stderr_hash_mismatch"
    );
    ensure!(
        stdout_hash == expected.stdout_sha256
            && record["stdout_sha256"] == stdout_hash
            && process["stdout"]["sha256"] == stdout_hash,
        "geometry_stdout_hash_mismatch"
    );
    match oe_core::json::strict_json(&stdout) {
        Ok(value) if value.is_object() => {
            ensure!(
                value == expected.output,
                "geometry_result_projection_changed"
            );
        }
        _ => errors.push("geometry_stdout_protocol_invalid".to_owned()),
    }
    ensure!(
        record["elapsed_seconds"].as_f64() == Some(expected.elapsed_seconds),
        "geometry_result_elapsed_changed"
    );
    let snapshot = record["input_snapshot"]
        .as_str()
        .context("geometry_input_snapshot_missing")?;
    let snapshot_path = Path::new(snapshot);
    ensure!(
        snapshot_path.starts_with(directory.join("_oe/input")),
        "geometry_input_snapshot_outside_job"
    );
    let input: FileIdentity = serde_json::from_value(record["input_identity"].clone())?;
    ensure!(
        input.sha256 == expected.input_sha256
            && fingerprint_file(snapshot_path, crate::intake::MAX_STEP_BYTES)? == input,
        "geometry_input_snapshot_hash_mismatch"
    );
    let expected_files: Vec<ArtifactIdentity> = serde_json::from_value(record["outputs"].clone())?;
    let actual = artifacts(directory, &name)?;
    if actual.is_empty() {
        errors.push("geometry_no_output_artifacts".to_owned());
    }
    if actual != expected_files {
        errors.push("geometry_output_manifest_mismatch".to_owned());
    }
    let output_sha256 = artifacts_sha256(&actual)?;
    if record["outputs_sha256"] != output_sha256 {
        errors.push("geometry_output_manifest_hash_mismatch".to_owned());
    }
    Ok(AcceptanceResult {
        job_id: expected.job_id.clone(),
        status: if errors.is_empty() {
            "accepted"
        } else {
            "rejected"
        }
        .to_owned(),
        output_sha256,
        errors,
    })
}
