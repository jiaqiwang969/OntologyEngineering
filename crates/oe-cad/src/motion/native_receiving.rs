//! Bounded motion reception from original fetched jobs. The production CLI
//! additionally binds the current Project through the existing kernel owner.
//! No reception result grants adoption or updates Project decisions.
use anyhow::{Context, Result, ensure};
use oe_contracts::*;
use oe_core::{SkillRoot, sha256};
use oe_nx_worker::{assembly_motion, motion_evidence, motion_source};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
};
mod reopen;
mod source;
pub use reopen::receive_reopen;
use source::{ReceivedJob, Sources};

const RECORD: usize = 16 * 1024 * 1024;

fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    Ok(serde_json::from_value(oe_core::json::strict_json(bytes)?)?)
}
fn exact(value: &Value, fields: &[&str]) -> Result<()> {
    let o = value.as_object().context("motion_record_object_required")?;
    ensure!(
        o.len() == fields.len() && fields.iter().all(|f| o.contains_key(*f)),
        "motion_record_fields_differ"
    );
    Ok(())
}
fn job_source_fields(value: &Value) -> Result<()> {
    exact(
        value,
        &[
            "job",
            "fetch_snapshot",
            "fetch_completion",
            "native_result_ref",
        ],
    )
}
fn pointer(source: &ArtifactReference) -> CadSourcePointer1 {
    CadSourcePointer1 {
        source: source.clone(),
        pointer: String::new(),
    }
}
fn claims(values: &[String]) -> Result<()> {
    ensure!(
        values.len() <= 128
            && values.iter().collect::<BTreeSet<_>>().len() == values.len()
            && values
                .iter()
                .all(|s| !s.is_empty() && s.len() <= 1024 && !s.chars().any(char::is_control)),
        "motion_requested_claims_invalid"
    );
    Ok(())
}
fn base_report(
    reference: &ArtifactReference,
    r: &CadMotionReceiveRequest1,
) -> CadMotionReceiveResult1 {
    let mut evidence_refs = vec![
        pointer(reference),
        pointer(&r.project),
        pointer(&r.execution.job),
        pointer(&r.execution.fetch_snapshot),
    ];
    if let Some(v) = &r.execution.fetch_completion {
        evidence_refs.push(pointer(v));
    }
    CadMotionReceiveResult1 {
        schema: "oev2.cad-motion-receive-result/1".into(),
        request: reference.clone(),
        object: r.object.clone(),
        outcome: "unknown".into(),
        evidence_refs,
        transport_state: "unknown".into(),
        stream_state: "not_checked".into(),
        motion_state: "not_checked".into(),
        pair_identity_state: "not_checked".into(),
        query_state: "not_checked".into(),
        save_state: "not_checked".into(),
        reopen_state: "not_checked".into(),
        locally_checked_samples: 0,
        locally_checked_pairs: 0,
        interference_pairs: 0,
        no_interference_pairs: 0,
        touching_pairs: 0,
        received_scope: vec![],
        unresolved_claims: r.requested_claims.clone(),
        issues: vec![],
        diagnostic_pages: vec![],
        engineering_adoption: "not_assessed".into(),
    }
}
fn scope(report: &mut CadMotionReceiveResult1, requested: &[String]) {
    report.unresolved_claims = requested
        .iter()
        .filter(|v| !report.received_scope.contains(v))
        .cloned()
        .collect();
}

fn received_state(raw: &str) -> Result<String> {
    Ok(match raw {
        "verified_sealed" | "checked_discrete" | "checked_complete" => "received",
        "verified_prefix"
        | "verified_prefix_partial_line"
        | "partial"
        | "failed"
        | "checked_raw_identity_incomplete" => "partial",
        "unknown" | "unknown_result" => "unknown",
        "not_started" => "not_checked",
        _ => anyhow::bail!("motion_core_state_unknown:{raw}"),
    }
    .into())
}

struct MotionInputs {
    params: NxAssemblyMotionParams2,
    intent: NxAssemblyMotionIntent,
    closure: NxAssemblySourceClosure,
    binding: NxMotionEvidenceBinding,
    parameters: ArtifactReference,
}
fn inputs(job: &ReceivedJob, request: &CadMotionReceiveRequest1) -> Result<MotionInputs> {
    ensure!(
        job.job.action == "run_assembly_motion",
        "motion_job_action_differs"
    );
    let raw = job.bytes("params.json", RECORD)?;
    let mut v = oe_core::json::strict_json(&raw)?;
    exact(&v, &["schema", "action", "intent", "budget", "evidence"])?;
    let schema = v["schema"]
        .as_str()
        .context("motion_params_schema_missing")?;
    ensure!(
        job.job.params_schema == schema,
        "motion_job_params_schema_differs"
    );
    match request.capture_scope.as_str() {
        "binding_facts2" => ensure!(
            schema == "oev2.nx-assembly-motion-params/2",
            "motion_capture_version_mismatch"
        ),
        "legacy_observation1" => {
            ensure!(
                schema == "oev2.nx-assembly-motion-params/1",
                "motion_capture_version_mismatch"
            );
            // Local common shape only; the original /1 schema and bytes remain
            // bound and are never presented to the /2 semantic receiver.
            v["schema"] = json!("oev2.nx-assembly-motion-params/2");
        }
        _ => anyhow::bail!("motion_capture_scope_unknown"),
    }
    let params: NxAssemblyMotionParams2 = serde_json::from_value(v)?;
    params
        .validate()
        .map_err(|_| anyhow::anyhow!("motion_params_invalid"))?;
    let intent: NxAssemblyMotionIntent = decode(&job.input(&params.intent, 32 * 1024 * 1024)?)?;
    assembly_motion::validate_intent(&intent)?;
    ensure!(
        intent.object == request.object,
        "motion_request_object_differs"
    );
    let closure: NxAssemblySourceClosure =
        decode(&job.input(&intent.assembly_source, 4 * 1024 * 1024)?)?;
    motion_source::validate_source_closure(&closure)?;
    ensure!(
        intent.root_part == closure.root_part,
        "motion_source_root_differs"
    );
    for f in &closure.files {
        ensure!(
            f.path.to_ascii_lowercase().ends_with(".prt"),
            "motion_native_source_required"
        );
        let name = format!("inputs/{}", f.path);
        let entry = job
            .job
            .inputs
            .iter()
            .find(|v| v.path.as_str() == name)
            .context("motion_native_source_not_in_job")?;
        ensure!(
            entry.identity.bytes == f.bytes && entry.identity.sha256.as_str() == f.sha256,
            "motion_native_source_identity_differs"
        );
    }
    let binding = NxMotionEvidenceBinding {
        job_id: job.job.job_id.as_str().into(),
        object: intent.object.clone(),
        intent: params.intent.clone(),
        samples: intent.samples.len() as u64,
    };
    motion_evidence::validate_stream_inputs(&request.receiver_budget, &binding)?;
    let parameters = ArtifactReference {
        path: "params.json".into(),
        sha256: sha256(&raw),
    };
    Ok(MotionInputs {
        params,
        intent,
        closure,
        binding,
        parameters,
    })
}

fn saved_position(job: &ReceivedJob) -> Result<Option<NxAssemblyPositionObservation>> {
    let Some(readback) = &job.readback else {
        return Ok(None);
    };
    let Some(value) = readback.get("saved_position") else {
        return Ok(None);
    };
    let r: ArtifactReference = serde_json::from_value(value.clone())?;
    ensure!(
        r.path == "out/motion-saved-position.json",
        "motion_saved_position_locator_differs"
    );
    let raw = job.bytes(&r.path, 32 * 1024 * 1024)?;
    ensure!(
        sha256(&raw) == r.sha256,
        "motion_saved_position_digest_differs"
    );
    Ok(Some(decode(&raw)?))
}
fn names(job: &ReceivedJob) -> Vec<String> {
    job.snapshot
        .files
        .iter()
        .filter_map(|f| {
            f.entry
                .path
                .as_str()
                .strip_prefix("out/motion-evidence/")
                .map(str::to_owned)
        })
        .collect()
}
fn owned_paths(
    job: &ReceivedJob,
    closure: &NxAssemblySourceClosure,
) -> Result<BTreeMap<String, String>> {
    let namespace = job.namespace(&closure.root_part, closure.files.len())?;
    let slash = if namespace.contains('\\') { "\\" } else { "/" };
    Ok(closure
        .files
        .iter()
        .map(|f| {
            (
                format!("{namespace}{slash}{}", f.path.replace('/', slash)),
                f.path.clone(),
            )
        })
        .collect())
}
fn abi(job: &ReceivedJob) -> Result<motion_evidence::receiving2::NativeBodyAbi> {
    let candidates: Vec<_> = job
        .snapshot
        .files
        .iter()
        .filter(|f| {
            let name = f.entry.path.as_str();
            name.starts_with("receipts/") && name.ends_with("-motion_compiled_abi2.json")
        })
        .collect();
    ensure!(
        candidates.len() == 1,
        "motion_compiled_abi_receipt_missing_or_ambiguous"
    );
    let v = oe_core::json::strict_json(&job.bytes(candidates[0].entry.path.as_str(), RECORD)?)?;
    ensure!(
        v["schema"] == "cad-agent.nx-native-event/v2"
            && v["job_id"] == job.job.job_id.as_str()
            && v["phase"] == "motion_compiled_abi2"
            && v["sequence"].as_u64().is_some_and(|s| s > 0),
        "motion_abi_receipt_binding_differs"
    );
    let o = &v["observation"];
    exact(
        o,
        &[
            "schema",
            "abi_version",
            "solid_type",
            "solid_subtype",
            "solid_body",
        ],
    )?;
    let dto: NxMotionCompiledAbi2 = serde_json::from_value(o.clone())?;
    dto.validate()
        .map_err(|_| anyhow::anyhow!("motion_compiled_abi_invalid"))?;
    ensure!(
        dto.abi_version == 2 && dto.solid_subtype == 0,
        "motion_compiled_body_subtype_unsupported"
    );
    Ok(motion_evidence::receiving2::NativeBodyAbi {
        solid_type: i32::try_from(dto.solid_type)?,
        solid_body: i32::try_from(dto.solid_body)?,
    })
}
fn project_result_binding(
    sources: &mut Sources<'_>,
    request: &CadMotionReceiveRequest1,
    job: &ReceivedJob,
) -> Result<()> {
    let project = oe_core::json::strict_json(&sources.read(&request.project, RECORD)?)?;
    // The kernel owns Project interpretation. This narrow check binds an
    // explicitly supplied evidence ID to the actual raw native-result bytes.
    if let Some(id) = &request.execution.native_result_ref {
        ensure!(
            !id.is_empty() && id.len() <= 1024,
            "motion_native_result_ref_invalid"
        );
        let raw = job.bytes("native-result.json", RECORD)?;
        ensure!(
            project["evidence"][id] == sha256(&raw),
            "motion_native_result_project_digest_differs"
        );
    }
    Ok(())
}

fn receive_inner(
    sources: &mut Sources<'_>,
    reference: &ArtifactReference,
) -> Result<(
    CadMotionReceiveResult1,
    CadMotionReceiveRequest1,
    ReceivedJob,
    MotionInputs,
)> {
    let bytes = sources.read(reference, RECORD)?;
    let value = oe_core::json::strict_json(&bytes)?;
    exact(
        &value,
        &[
            "schema",
            "project",
            "object",
            "project_context",
            "execution",
            "capture_scope",
            "receiver_budget",
            "requested_claims",
        ],
    )?;
    job_source_fields(&value["execution"])?;
    let request: CadMotionReceiveRequest1 = serde_json::from_value(value)?;
    request
        .validate()
        .map_err(|_| anyhow::anyhow!("motion_receive_request_invalid"))?;
    claims(&request.requested_claims)?;
    let job = source::receive(sources, &request.execution)?;
    project_result_binding(sources, &request, &job)?;
    let inputs = inputs(&job, &request)?;
    let mut report = base_report(reference, &request);
    report.transport_state = if job.fetch_complete {
        "received"
    } else {
        "unknown"
    }
    .into();
    if job.fetch_complete {
        report.received_scope.push("verified_job_bytes".into());
    }
    let names = names(&job);
    if names.is_empty() && !job.completed() {
        report
            .issues
            .push("motion_stream_not_recorded; native entry and outcome remain unknown".into());
        scope(&mut report, &request.requested_claims);
        job.recheck()?;
        sources.recheck()?;
        return Ok((report, request, job, inputs));
    }
    let summary: Option<NxMotionEvidenceSummary> = job
        .readback
        .as_ref()
        .and_then(|r| r.get("evidence"))
        .map(|v| serde_json::from_value(v.clone()))
        .transpose()?;
    let mut source =
        |name: &str, visit: &mut dyn FnMut(&mut dyn std::io::Read) -> Result<()>| -> Result<()> {
            let path = job.member(&format!("out/motion-evidence/{name}"))?;
            let mut file = File::open(&path)?;
            visit(&mut file)?;
            // Entire pinned inventory is independently rehashed after reception.
            oe_core::no_symlinks(&path)?;
            Ok(())
        };
    if request.capture_scope == "legacy_observation1" {
        if let Some(summary) = summary.as_ref() {
            motion_evidence::receiving::verify_evidence_with(
                summary,
                &request.receiver_budget,
                &inputs.binding,
                &names,
                &mut source,
                &mut |event| {
                    ensure!(
                        event.phase != "motion_capture" && event.phase != "motion_body_binding",
                        "motion_legacy_capture_version_mismatch"
                    );
                    Ok(())
                },
            )?;
            report.stream_state = "received".into();
            report.pair_identity_state = "legacy_limited".into();
            report.motion_state = "legacy_limited".into();
            report.query_state = "legacy_limited".into();
            report.save_state = "legacy_limited".into();
            report.outcome = if job.completed() && job.fetch_complete {
                "accepted_bounded"
            } else {
                "partial"
            }
            .into();
            report.received_scope.push("legacy_stream_bytes".into());
            report.issues.push("legacy capture has no full native query facts; no /2 identity or engineering upgrade".into());
        } else {
            ensure!(!job.completed(), "motion_success_without_sealed_stream");
            report.stream_state = "unknown".into();
            report.issues.push("legacy stream incomplete".into());
        }
    } else {
        let owned = owned_paths(&job, &inputs.closure)?;
        let saved = saved_position(&job)?;
        let received = motion_evidence::receiving2::receive(
            motion_evidence::receiving2::Inputs2 {
                parameters: &inputs.parameters,
                params: &inputs.params,
                intent: &inputs.intent,
                closure: &inputs.closure,
                binding: &inputs.binding,
                receiver_budget: &request.receiver_budget,
                abi: abi(&job)?,
                owned_paths: &owned,
                completed: job.completed(),
                saved_position: saved.as_ref(),
            },
            summary.as_ref(),
            &names,
            &mut source,
        )?;
        report.outcome = received.outcome.into();
        report.stream_state = received_state(received.stream_state)?;
        report.motion_state = received_state(received.motion_state)?;
        report.pair_identity_state = received_state(received.pair_identity_state)?;
        report.query_state = received_state(received.query_state)?;
        report.save_state = match received.save_state {
            motion_evidence::receiving2::SaveState::NotStarted => "not_checked",
            motion_evidence::receiving2::SaveState::PendingUnknown => "unknown",
            motion_evidence::receiving2::SaveState::Failed
            | motion_evidence::receiving2::SaveState::Incomplete => "partial",
            motion_evidence::receiving2::SaveState::CallSucceeded => "unknown",
        }
        .into();
        report.locally_checked_samples = received.samples;
        report.locally_checked_pairs = received.pairs;
        report.interference_pairs = received.interference;
        report.no_interference_pairs = received.no_interference;
        report.touching_pairs = received.touching;
        report.issues = received.issues;
        report.issues.push(format!(
            "recorded_receiver_states:stream={},motion={},identity={},query={},save={}",
            received.stream_state,
            received.motion_state,
            received.pair_identity_state,
            received.query_state,
            received.save_state.as_str()
        ));
        if report.stream_state == "received" {
            report.received_scope.push("sealed_motion_stream".into());
        }
        if report.motion_state == "received" {
            report
                .received_scope
                .push("discrete_component_positions".into());
        }
        if report.pair_identity_state == "received" && report.query_state == "received" {
            report
                .received_scope
                .push("discrete_native_pair_checks".into());
        }
        if received.save_state == motion_evidence::receiving2::SaveState::CallSucceeded
            && received.saved_position_checked
            && saved.is_some()
        {
            for f in &inputs.closure.files {
                let name = format!("out/motion-assembly/{}", f.path);
                job.member(&name)?;
                ensure!(
                    job.result
                        .as_ref()
                        .is_some_and(|r| r.artifacts.iter().any(|a| a.path.as_str() == name)),
                    "motion_saved_closure_member_missing"
                );
            }

            report.save_state = "received".into();
            report
                .received_scope
                .push("native_save_and_saved_position".into());
        }
        if !job.fetch_complete && report.outcome == "accepted_bounded" {
            report.outcome = "partial".into();
            report.issues.push("fetch completion absent".into());
        }
    }
    scope(&mut report, &request.requested_claims);
    job.recheck()?;
    sources.recheck()?;
    Ok((report, request, job, inputs))
}

/// Requires the production caller to apply the existing kernel's current
/// Project context binding before and after this byte/observation reception.
pub fn receive(root: &SkillRoot, request: &ArtifactReference) -> Result<Value> {
    let mut sources = Sources::new(root);
    let (report, _, _, _) = receive_inner(&mut sources, request)?;
    report
        .validate()
        .map_err(|_| anyhow::anyhow!("motion_receive_report_invalid"))?;
    sources.recheck()?;
    Ok(serde_json::to_value(report)?)
}
