//! Read-only SolidWorks external evidence admission. Native execution is separate.
use crate::evidence::{self, common};
use anyhow::{Context, Result, ensure};
use oe_contracts::SolidWorksEvidenceRequest;
use oe_core::{SkillRoot, read_regular_bytes, sha256};
use serde_json::{Value, json};
use std::path::Path;

pub fn capabilities() -> Value {
    json!({"schema":"oev2.cad-backend-capabilities/1","backend":"solidworks",
    "actions":[{"action":"verify_external_evidence","implementation":"Rust","state":"implemented"}],
    "native_worker":"Rust_finite_worker_implemented_requires_current_native_validation","native_worker_contract":oe_sw_worker::capabilities(),"external_adapter":"third_party_python_mcp",
    "remote_availability":"not_observed","native_process_reopen":"not_qualified",
    "step_roundtrip":"not_qualified","assembly":"not_qualified","manufacturing_drawing":"not_qualified",
    "nx_backend":"retained","engineering_adoption":"not_assessed"})
}
/// Checks original record bytes; does not authenticate arbitrary JSON as native
/// execution. Admission remains with the existing evidence packet and Project.
pub fn verify_native_record(raw_request: &[u8], raw_observation: &[u8]) -> Result<Value> {
    let request = serde_json::from_value(oe_core::json::strict_json(raw_request)?)?;
    let observation = serde_json::from_value(oe_core::json::strict_json(raw_observation)?)?;
    let mut report = oe_sw_worker::check_observation(&request, raw_request, &observation)?;
    report["request_sha256"] = json!(sha256(raw_request));
    report["observation_sha256"] = json!(sha256(raw_observation));
    report["project"] = serde_json::to_value(&request.project)?;
    report["work"] = serde_json::to_value(&request.work)?;
    report["observed_execution"] = json!(observation.execution);
    report["handoff"] = json!(
        "existing cad evidence/handoff packet with original request, observation, COM trace and native model sources; same Project/work; no new ledger"
    );
    Ok(report)
}
pub fn verify_native_reopen(raws: [&[u8]; 4]) -> Result<Value> {
    let first = serde_json::from_value(oe_core::json::strict_json(raws[0])?)?;
    let saved = serde_json::from_value(oe_core::json::strict_json(raws[1])?)?;
    let second = serde_json::from_value(oe_core::json::strict_json(raws[2])?)?;
    let reopened = serde_json::from_value(oe_core::json::strict_json(raws[3])?)?;
    let mut report =
        oe_sw_worker::check_reopen(&first, raws[0], &saved, &second, raws[2], &reopened)?;
    report["original_sha256"] = json!(raws.map(sha256));
    report["project"] = serde_json::to_value(&first.project)?;
    report["work"] = serde_json::to_value(&first.work)?;
    Ok(report)
}
pub fn verify(root: &SkillRoot, request: &SolidWorksEvidenceRequest) -> Result<Value> {
    request
        .validate()
        .map_err(|_| anyhow::anyhow!("solidworks_generated_contract_invalid"))?;
    ensure!(
        request.schema == "oev2.solidworks-evidence/1",
        "solidworks_evidence_schema"
    );
    let packet_path = root.expand(Path::new(&request.packet.path))?;
    let project_root = root.expand(Path::new(&request.project_root))?;
    ensure!(
        packet_path.starts_with(root.path().join("var"))
            && project_root.starts_with(root.path().join("var")),
        "solidworks_private_source_required"
    );
    let raw = read_regular_bytes(&packet_path, 16 * 1024 * 1024)?;
    ensure!(
        sha256(&raw) == request.packet.sha256,
        "solidworks_packet_changed"
    );
    let verified = evidence::verify_bytes(&raw, &project_root)?;
    let packet = verified.packet();
    let source = |id: &str| -> Result<&Value> {
        packet["sources"]
            .as_array()
            .context("solidworks_sources_missing")?
            .iter()
            .find(|s| s["id"] == id)
            .context("solidworks_source_id_missing")
    };
    let deployment = source(&request.deployment_source_id)?;
    let native = source(&request.native_model_source_id)?;
    ensure!(
        request.deployment_source_id != request.native_model_source_id
            && deployment["media_type"] == "application/json"
            && native["role"] == "native_model",
        "solidworks_source_role_mismatch"
    );
    let dep_path = common::path(&project_root, common::s(deployment, "path"))?;
    let dep_raw = read_regular_bytes(&dep_path, 16 * 1024 * 1024)?;
    ensure!(
        sha256(&dep_raw) == common::s(deployment, "sha256"),
        "solidworks_deployment_changed"
    );
    let receipt = oe_core::json::strict_json(&dep_raw)?;
    ensure!(
        receipt["repository"] == "https://github.com/czuryk/SolidworksMCP",
        "solidworks_external_adapter_unrecognized"
    );
    let commit = receipt["commit"]
        .as_str()
        .context("solidworks_commit_missing")?;
    ensure!(
        commit.len() == 40 && commit.bytes().all(|c| c.is_ascii_hexdigit()),
        "solidworks_commit_invalid"
    );
    let native_path = Path::new(common::s(native, "path"));
    let filename = native_path
        .file_name()
        .and_then(|s| s.to_str())
        .context("solidworks_native_filename_invalid")?;
    ensure!(
        native_path
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.eq_ignore_ascii_case("sldprt")
                || s.eq_ignore_ascii_case("sldasm")
                || s.eq_ignore_ascii_case("slddrw")),
        "solidworks_native_extension_invalid"
    );
    ensure!(
        receipt["sha256"].get(filename) == Some(&native["sha256"]),
        "solidworks_native_not_bound_to_deployment_receipt"
    );
    // Preserve the original claims as claims. No Python repr is evaluated or promoted.
    let report = json!({"schema":"oev2.solidworks-evidence-verification/1","backend":"solidworks",
        "packet_sha256":request.packet.sha256,"project":packet["project"],"source_integrity":verified.audit(),
        "external_deployment_receipt":receipt,"external_deployment_source_id":request.deployment_source_id,
        "native_model_source_id":request.native_model_source_id,"adapter_kind":"third_party_python_mcp",
        "current_native_execution":"not_run","independent_cad_process_reopen":"not_established",
        "step_roundtrip":"not_established","current_project_review":"not_run",
        "semantic_execution":"not_run","engineering_adoption":"not_assessed",
        "handoff":"existing cad evidence packet and same project/configuration/revision/state; no second ledger"});
    ensure!(
        read_regular_bytes(&packet_path, 16 * 1024 * 1024)? == raw,
        "solidworks_packet_changed"
    );
    let checked_again = evidence::verify_bytes(&raw, &project_root)?;
    ensure!(
        checked_again.audit() == verified.audit(),
        "solidworks_sources_changed"
    );
    ensure!(
        read_regular_bytes(&dep_path, 16 * 1024 * 1024)? == dep_raw,
        "solidworks_deployment_changed"
    );
    Ok(report)
}
