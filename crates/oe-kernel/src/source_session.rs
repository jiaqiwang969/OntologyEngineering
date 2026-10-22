//! Existing engineering kernel delegates the explicit neutral local session.
//! This adapter adds transport identity checks, never a second state engine.
use anyhow::{Result, ensure};
use semantica_contracts::{
    SemanticRdfLibDatasetSnapshot, SemanticSourceDatasetDiff, SemanticSourceSerialization,
    SemanticSourceSessionRequest, SemanticSourceSessionRequest2, SemanticSourceSessionRequest3,
    SemanticSourceSessionRequest4, SemanticSourceSessionResult, SemanticSourceSessionResult2,
    SemanticSourceSessionResult3, SemanticSourceSessionResult4,
};
pub use semantica_runtime::dataset::source_session::{
    DEADLINE_MS, MAX_OUTPUT_BYTES, MAX_WIRE_BYTES, request_sha256, request_sha256_2,
    request_sha256_3, request_sha256_4, source_bindings_sha256, source_bindings_sha256_2,
    source_bindings_sha256_3, source_bindings_sha256_4, validate_request, validate_request2,
    validate_request3, validate_request4,
};

pub fn execute_local_source_session(
    input: &SemanticSourceSessionRequest,
) -> Result<SemanticSourceSessionResult, semantica_contracts::SemanticRuntimeFailure> {
    semantica_runtime::runtime::RustSemanticRuntime.execute_source_session(input)
}

fn validate_state(value: &serde_json::Value, failure: bool, revision: u64) -> Result<()> {
    if failure {
        ensure!(value.is_null(), "source_session_worker_state_and_error");
        return Ok(());
    }
    ensure!(
        value.is_object()
            && value["revision"].as_u64() == Some(revision)
            && value["contexts"].is_array()
            && value["quads"].is_array()
            && value["quad_count"].as_u64().is_some()
            && value["query_cache_entries"].as_u64().is_some()
            && value["snapshot_cached"].is_boolean(),
        "source_session_worker_missing_state"
    );
    ensure!(
        value["quad_count"].as_u64() == Some(value["quads"].as_array().unwrap().len() as u64),
        "source_session_worker_state_coverage"
    );
    Ok(())
}

/// Verify a completed worker's complete response against the immutable request.
/// This is envelope coverage/identity validation, not re-execution of semantics.
// Both generated versions use the same receiving rules; this macro borrows
// their fields and avoids cloning complete result trees to erase a version.
macro_rules! validate_common_received {
    ($input:expr, $result:expr, $validate:path, $request_hash:path, $source_hash:path) => {
        validate_common_received!($input, $result, $validate, $request_hash, $source_hash, false, false)
    };
    ($input:expr, $result:expr, $validate:path, $request_hash:path, $source_hash:path, $initial_stopped:expr, $final_stopped:expr) => {{
        let input = $input;
        let result = $result;
        (|| -> Result<()> {
    $validate(input)?;
    ensure!(
        result.validate().is_ok(),
        "source_session_worker_result_invalid"
    );
    ensure!(
        result.request_sha256 == $request_hash(input)? && result.profile == input.profile,
        "source_session_worker_identity_mismatch"
    );
    ensure!(
        result.source_bindings_sha256 == $source_hash(input)?,
        "source_session_worker_source_mismatch"
    );
    ensure!(
        result.steps.len() == input.actions.len(),
        "source_session_worker_action_coverage"
    );
    ensure!(
        result.initial_result
            == serde_json::json!({"profile":"rdf","cache_queries":input.cache_queries}),
        "source_session_worker_initial_identity"
    );
    validate_state(
        &result.initial_state,
        result.initial_state_failure.is_some(),
        0,
    )?;
    let capture_stopped = result.status == "partial"
        && result
            .failure
            .as_ref()
            .is_some_and(|e| e.code == "source_session_result_byte_budget" && e.phase == "capture");
    let mut revision = 0;
    let mut failed = $initial_stopped;
    let mut skipped = false;
    for (action, step) in input.actions.iter().zip(&result.steps) {
        ensure!(
            step.validate().is_ok() && step.id == action.id && step.operation == action.operation,
            "source_session_worker_action_identity"
        );
        ensure!(
            step.revision_before == revision,
            "source_session_worker_revision_chain"
        );
        revision = step.revision_after;
        if action.operation == "update" && step.status != "skipped" {
            ensure!(
                ["not_entered", "reached"].contains(&step.backend_phase.as_str()),
                "source_session_worker_update_phase"
            );
        } else {
            ensure!(
                step.backend_phase == "not_applicable",
                "source_session_worker_nonupdate_phase"
            );
        }
        match step.status.as_str() {
            "completed" => {
                ensure!(
                    step.after_state.is_none() && step.after_state_failure.is_none(),
                    "source_session_worker_completed_failure_observation"
                );
                if action.operation == "update" {
                    ensure!(
                        step.backend_phase == "reached",
                        "source_session_worker_unreached_success"
                    );
                }
                ensure!(
                    step.failure.is_none() && !step.result.is_null(),
                    "source_session_worker_completed_shape"
                );
                ensure!(
                    !(failed && matches!(action.operation.as_str(), "load" | "update" | "clear")),
                    "source_session_worker_mutation_after_failure"
                );
            }
            "failed" => {
                ensure!(
                    step.failure.is_some() && step.result.is_null(),
                    "source_session_worker_failure_shape"
                );
                failed = true;
            }
            "skipped" => {
                ensure!(
                    failed
                        && step.failure.is_none()
                        && step.result.is_null()
                        && step.revision_after == step.revision_before,
                    "source_session_worker_skipped_shape"
                );
                ensure!(
                    matches!(action.operation.as_str(), "load" | "update" | "clear")
                        || capture_stopped,
                    "source_session_worker_read_observation_skipped"
                );
                ensure!(
                    step.source_state.is_null()
                        && step.source_state_failure.is_none()
                        && step.after_state.is_none()
                        && step.after_state_failure.is_none(),
                    "source_session_worker_skipped_observation"
                );
                skipped = true;
            }
            _ => anyhow::bail!("source_session_worker_status_invalid"),
        }
        if step.status != "skipped" {
            validate_state(
                &step.source_state,
                step.source_state_failure.is_some(),
                revision,
            )?;
        }
        ensure!(
            !(step.after_state.is_some() && step.after_state_failure.is_some()),
            "source_session_worker_observation_ambiguous"
        );
        if step.status == "failed" {
            let capture_unavailable = capture_stopped
                && step
                    .source_state_failure
                    .as_ref()
                    .is_some_and(|e| e.code == "source_session_result_byte_budget");
            ensure!(
                step.after_state.is_some()
                    || step.after_state_failure.is_some()
                    || capture_unavailable,
                "source_session_worker_failure_observation_missing"
            );
        }
        if let Some(state) = &step.after_state {
            ensure!(
                state.revision == revision,
                "source_session_worker_state_revision"
            );
        }
    }
    ensure!(
        !(result.final_state.is_some() && result.final_state_failure.is_some()),
        "source_session_worker_final_ambiguous"
    );
    ensure!(
        result.final_state.is_some() || result.final_state_failure.is_some(),
        "source_session_worker_final_missing"
    );
    if let Some(state) = &result.final_state {
        ensure!(
            state.revision == revision,
            "source_session_worker_final_revision"
        );
    }
    match result.status.as_str() {
        "completed" => ensure!(
            !failed
                && !skipped
                && result.failure.is_none()
                && result.final_state.is_some()
                && result.initial_state_failure.is_none()
                && result.steps.iter().all(|s| s.source_state_failure.is_none()
                    && s.after_state_failure.is_none()
                    && s.resource_requests_complete),
            "source_session_worker_false_completed"
        ),
        "failed" => ensure!(
            failed
                || result.final_state_failure.is_some()
                || result.initial_state_failure.is_some()
                || result
                    .steps
                    .iter()
                    .any(|s| s.source_state_failure.is_some() || !s.resource_requests_complete),
            "source_session_worker_false_failed"
        ),
        "partial" => ensure!(
            (failed || $final_stopped) && capture_stopped,
            "source_session_worker_partial_reason_missing"
        ),
        _ => anyhow::bail!("source_session_worker_status_invalid"),
    }
    Ok(())

        })()
    }};
}
pub fn validate_received(
    input: &SemanticSourceSessionRequest,
    result: &SemanticSourceSessionResult,
) -> Result<()> {
    validate_common_received!(
        input,
        result,
        validate_request,
        request_sha256,
        source_bindings_sha256
    )
}
pub fn execute_local_source_session2(
    input: &SemanticSourceSessionRequest2,
) -> Result<SemanticSourceSessionResult2, semantica_contracts::SemanticRuntimeFailure> {
    semantica_runtime::runtime::RustSemanticRuntime.execute_source_session2(input)
}
pub fn validate_received2(
    input: &SemanticSourceSessionRequest2,
    result: &SemanticSourceSessionResult2,
) -> Result<()> {
    validate_common_received!(
        input,
        result,
        validate_request2,
        request_sha256_2,
        source_bindings_sha256_2
    )?;
    for (index, (action, step)) in input.actions.iter().zip(&result.steps).enumerate() {
        if step.status != "completed" {
            continue;
        }
        match action.operation.as_str() {
            "serialize" => {
                let value: SemanticSourceSerialization =
                    serde::Deserialize::deserialize(&step.result)?;
                ensure!(
                    value.validate().is_ok(),
                    "source_session_worker_serialize_invalid"
                );
                ensure!(
                    step.revision_before == step.revision_after,
                    "source_session_worker_storage_revision"
                );
            }
            "diff" => {
                let value: SemanticSourceDatasetDiff =
                    serde::Deserialize::deserialize(&step.result)?;
                ensure!(
                    value.validate().is_ok(),
                    "source_session_worker_diff_invalid"
                );
                let endpoint = |id: &str| -> Result<SemanticRdfLibDatasetSnapshot> {
                    let prior = result.steps[..index]
                        .iter()
                        .find(|s| s.id == id)
                        .ok_or_else(|| {
                            anyhow::anyhow!("source_session_worker_diff_endpoint_missing")
                        })?;
                    ensure!(
                        prior.operation == "snapshot"
                            && prior.status == "completed"
                            && prior.failure.is_none()
                            && !prior.result.is_null(),
                        "source_session_worker_diff_endpoint_not_retained"
                    );
                    let snapshot: SemanticRdfLibDatasetSnapshot =
                        serde::Deserialize::deserialize(&prior.result)?;
                    ensure!(
                        snapshot.validate().is_ok(),
                        "source_session_worker_diff_endpoint_invalid"
                    );
                    Ok(snapshot)
                };
                let before = endpoint(action.before_snapshot_action_id.as_deref().unwrap())?;
                ensure!(
                    value.before_sha256 == before.dataset_sha256,
                    "source_session_worker_diff_before_identity"
                );
                if let Some(id) = &action.after_snapshot_action_id {
                    ensure!(
                        value.after_sha256 == endpoint(id)?.dataset_sha256,
                        "source_session_worker_diff_after_identity"
                    );
                }
                ensure!(
                    step.revision_before == step.revision_after,
                    "source_session_worker_storage_revision"
                );
            }
            _ => {}
        }
    }
    Ok(())
}

/// Explicit strengthened relation contract. The old versions retain their exact
/// receiving scope; this never treats a self-consistent witness as provenance.
pub fn execute_local_source_session3(
    input: &SemanticSourceSessionRequest3,
) -> Result<SemanticSourceSessionResult3, semantica_contracts::SemanticRuntimeFailure> {
    semantica_runtime::runtime::RustSemanticRuntime.execute_source_session3(input)
}
/// Standalone entry with an independent bounded receiving budget. Production
/// host transport must use validate_received3_until with its original deadline.
pub fn validate_received3(
    input: &SemanticSourceSessionRequest3,
    result: &SemanticSourceSessionResult3,
) -> Result<()> {
    validate_received3_until(
        input,
        result,
        std::time::Instant::now() + std::time::Duration::from_millis(DEADLINE_MS),
    )
}
pub fn validate_received3_until(
    input: &SemanticSourceSessionRequest3,
    result: &SemanticSourceSessionResult3,
    deadline: std::time::Instant,
) -> Result<()> {
    ensure!(
        std::time::Instant::now() < deadline,
        "source_session_receiving_deadline"
    );
    validate_common_received!(
        input,
        result,
        validate_request3,
        request_sha256_3,
        source_bindings_sha256_3
    )?;
    semantica_runtime::dataset::source_session::validate_storage_relations3_until(
        input, result, deadline,
    )
}

/// Explicit phased observation/writer contract on the existing source owner.
pub fn execute_local_source_session4(
    input: &SemanticSourceSessionRequest4,
) -> Result<SemanticSourceSessionResult4, semantica_contracts::SemanticRuntimeFailure> {
    semantica_runtime::runtime::RustSemanticRuntime.execute_source_session4(input)
}
pub fn validate_received4(
    input: &SemanticSourceSessionRequest4,
    result: &SemanticSourceSessionResult4,
) -> Result<()> {
    validate_received4_until(
        input,
        result,
        std::time::Instant::now() + std::time::Duration::from_millis(DEADLINE_MS),
    )
}
pub fn validate_received4_until(
    input: &SemanticSourceSessionRequest4,
    result: &SemanticSourceSessionResult4,
    deadline: std::time::Instant,
) -> Result<()> {
    ensure!(
        std::time::Instant::now() < deadline,
        "source_session_receiving_deadline"
    );
    validate_common_received!(
        input,
        result,
        validate_request4,
        request_sha256_4,
        source_bindings_sha256_4,
        result
            .initial_state_failure
            .as_ref()
            .is_some_and(|f| f.phase == "capture"),
        result.trace.final_namespace.status == "failed"
    )?;
    semantica_runtime::dataset::source_session::validate_relations4_until(input, result, deadline)
}
