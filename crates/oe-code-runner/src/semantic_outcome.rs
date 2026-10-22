//! Observe the existing reason port. No oracle/case IDs enter this adapter.
use anyhow::{Result, ensure};
use oe_contracts::CodeSemanticReplayOutcome;
use oe_wire::json::{canonical_json, ordered_json};
use semantica_contracts::{SemanticReasonPort, SemanticReasonRequest, SemanticRuntimeFailure};
use serde_json::Value;

fn condition(failure: &SemanticRuntimeFailure) -> Option<&'static str> {
    if failure.phase != "reason" || failure.code != "reason_execution_failed" || failure.retryable {
        return None;
    }
    match failure.message.as_str() {
        "runtime_reason_input: duplicate rule ID" => Some("duplicate_rule_id"),
        "runtime_reason_input: semantic duplicate" => Some("semantic_duplicate"),
        "runtime_reason_input: invalid rule ID" => Some("invalid_rule_id"),
        "runtime_reason_input: invalid ForwardRuleDTO wire fields" => Some("invalid_dto_fields"),
        "runtime_reason_input: confidence must be finite in [0,1]" => Some("confidence_range"),
        "runtime_reason_input: priority must be an integer" => Some("priority_integer"),
        "runtime_reason_input: invalid rule syntax" => Some("invalid_rule_syntax"),
        "runtime_reason_input: each rule item must contain exactly one line"
        | "runtime_reason_input: each rule must occupy one line" => Some("one_line_rule"),
        "runtime_reason_input: facts and rules are required" => Some("empty_facts_or_rules"),
        "runtime_reason_input: unbound conclusion variable" => Some("unbound_conclusion"),
        message if message.starts_with("runtime_reason_convergence: forward reasoning did not converge within max_iterations=") => Some("convergence"),
        _ => None,
    }
}

pub(crate) fn execute(input: &Value) -> Result<CodeSemanticReplayOutcome> {
    // The runner's existing whole-invocation 1 MiB read guard executes first.
    // This explicit inner guard prevents direct calls bypassing the port bound.
    ensure!(
        ordered_json(input)?.len() <= semantica_runtime::reasoner::MAX_INPUT_BYTES,
        "runner_reason_input_budget"
    );
    let request_sha256 = oe_core::sha256(canonical_json(input)?);
    let request: SemanticReasonRequest = match serde_json::from_value(input.clone()) {
        Ok(value) => value,
        Err(error) => {
            // Semantic decoding is observed only for this known top-level integer
            // type boundary. Other unknown wire/contract errors remain execution
            // failures; none can impersonate an oracle's expected rejection.
            let object = input
                .as_object()
                .ok_or_else(|| anyhow::anyhow!("runner_reason_request_object"))?;
            ensure!(
                object.len() == 4
                    && ["schema", "facts", "rules", "max_iterations"]
                        .iter()
                        .all(|key| object.contains_key(*key))
                    && input["schema"] == "semantica.reason-request/1"
                    && input["max_iterations"].is_boolean(),
                "runner_reason_unclassified_decode:{error}"
            );
            return Ok(CodeSemanticReplayOutcome {
                schema: "oev2.code-semantic-replay-outcome/1".into(),
                request_sha256,
                outcome: "rejected".into(),
                stage: "request_decode".into(),
                reason_port_invoked: false,
                result: None,
                failure: Some(SemanticRuntimeFailure {
                    phase: "request_decode".into(),
                    code: "invalid_reason_request".into(),
                    message: error.to_string(),
                    retryable: false,
                }),
                condition: Some("request_integer_type".into()),
            });
        }
    };
    let (outcome, result, failure, condition) =
        match semantica_runtime::runtime::RustSemanticRuntime.reason(&request) {
            Ok(result) => {
                ensure!(
                    result.request_sha256 == request_sha256,
                    "runner_reason_result_identity_changed"
                );
                ("success", Some(result), None, None)
            }
            Err(error) => {
                let detail = condition(&error).ok_or_else(|| {
                    anyhow::anyhow!(
                        "runner_reason_unclassified_failure:{}:{}:{}",
                        error.phase,
                        error.code,
                        error.message
                    )
                })?;
                ("rejected", None, Some(error), Some(detail.into()))
            }
        };
    let result = CodeSemanticReplayOutcome {
        schema: "oev2.code-semantic-replay-outcome/1".into(),
        request_sha256,
        outcome: outcome.into(),
        stage: "reason".into(),
        reason_port_invoked: true,
        result,
        failure,
        condition,
    };
    result
        .validate()
        .map_err(|errors| anyhow::anyhow!("runner_reason_outcome_contract:{errors:?}"))?;
    Ok(result)
}
