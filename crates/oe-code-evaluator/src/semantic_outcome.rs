//! Fixed outcome comparison, without a semantic runtime dependency.
use anyhow::{Result, ensure};
use oe_contracts::{CodeReplayCase, CodeSemanticReplayOutcome};
use oe_wire::json::canonical_json;
use serde_json::Value;

fn validate(value: &CodeSemanticReplayOutcome, case: &CodeReplayCase) -> Result<()> {
    value
        .validate()
        .map_err(|e| anyhow::anyhow!("evaluator_semantic_outcome_contract:{e:?}"))?;
    ensure!(
        value.request_sha256 == case.input_sha256,
        "evaluator_semantic_request_identity"
    );
    match value.outcome.as_str() {
        "success" => {
            let result = value
                .result
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("evaluator_semantic_result_missing"))?;
            result
                .validate()
                .map_err(|e| anyhow::anyhow!("evaluator_semantic_result_contract:{e:?}"))?;
            ensure!(
                value.stage == "reason"
                    && value.reason_port_invoked
                    && value.failure.is_none()
                    && value.condition.is_none()
                    && result.request_sha256 == value.request_sha256,
                "evaluator_semantic_success_shape"
            );
        }
        "rejected" => {
            let failure = value
                .failure
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("evaluator_semantic_failure_missing"))?;
            failure
                .validate()
                .map_err(|e| anyhow::anyhow!("evaluator_semantic_failure_contract:{e:?}"))?;
            ensure!(
                !failure.message.trim().is_empty()
                    && value.result.is_none()
                    && !failure.retryable
                    && value.condition.is_some(),
                "evaluator_semantic_rejection_shape"
            );
            match value.stage.as_str() {
                "request_decode" => ensure!(
                    !value.reason_port_invoked
                        && value.condition.as_deref() == Some("request_integer_type")
                        && failure.phase == "request_decode"
                        && failure.code == "invalid_reason_request"
                        && case.input["max_iterations"].is_boolean(),
                    "evaluator_semantic_decode_stage"
                ),
                "reason" => ensure!(
                    value.reason_port_invoked
                        && failure.phase == "reason"
                        && failure.code == "reason_execution_failed",
                    "evaluator_semantic_reason_stage"
                ),
                _ => anyhow::bail!("evaluator_semantic_unknown_stage"),
            }
        }
        _ => anyhow::bail!("evaluator_semantic_unknown_outcome"),
    }
    Ok(())
}

pub(crate) fn equivalent(case: &CodeReplayCase, value: &Value) -> Result<bool> {
    ensure!(
        case.profile == "local-semantic-replay/2"
            && case.capability == "semantic.reason-metadata/1"
            && case.comparison == "semantic_outcome/1"
            && case.expected_execution == "completed",
        "evaluator_semantic_outcome_profile"
    );
    let actual: CodeSemanticReplayOutcome = serde_json::from_value(value.clone())?;
    let expected: CodeSemanticReplayOutcome = serde_json::from_value(case.expected.clone())?;
    validate(&actual, case)?;
    validate(&expected, case)?;
    let mut actual = serde_json::to_value(actual)?;
    let expected = serde_json::to_value(expected)?;
    if expected["stage"] == "request_decode" {
        // Serde wording is diagnostic, not the semantic expectation. Typed
        // decoding stage, exact condition/code and original request still match.
        actual["failure"]["message"] = expected["failure"]["message"].clone();
    }
    Ok(canonical_json(&actual)? == canonical_json(&expected)?)
}
