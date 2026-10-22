//! Composition only: original typed dispatch reuses existing model-call and
//! BaseProvider algorithms, credential guard and sole shared call authority.
use super::model_calls::{SharedModelCallContext, generate_base_structured, generate_provider};
use oe_contracts::BaseStructuredRequest;
use semantica_contracts::*;
use serde_json::Value;

pub struct RustTypedDispatchHarness<'a> {
    calls: &'a SharedModelCallContext,
    transport: &'a dyn SemanticProviderTransportPort,
    guard: &'a dyn SemanticProviderResultGuardPort,
}
impl<'a> RustTypedDispatchHarness<'a> {
    pub fn new(
        calls: &'a SharedModelCallContext,
        transport: &'a dyn SemanticProviderTransportPort,
        guard: &'a dyn SemanticProviderResultGuardPort,
    ) -> Self {
        Self {
            calls,
            transport,
            guard,
        }
    }
}
impl SemanticTypedDispatchPort for RustTypedDispatchHarness<'_> {
    fn dispatch_typed(
        &self,
        input: &SemanticTypedDispatchRequest,
    ) -> Result<SemanticTypedDispatchResult, SemanticProviderFailure> {
        if input.validate().is_err() || input.schema != "semantica.typed-dispatch-request/1" {
            return Err(oe_semantic::providers::failure(
                "typed_dispatch_request_invalid",
                "validate",
            ));
        }
        let before = self.calls.snapshot().attempts;
        let mut observation = SemanticTypedDispatchResult {
            schema: "semantica.typed-dispatch-result/1".into(),
            execution: "failed".into(),
            output: Value::Null,
            response: None,
            reserved: false,
            failure: None,
        };
        if input.mode == "structured" {
            let base = generate_base_structured(
                self.calls,
                &BaseStructuredRequest {
                    schema: "oev2.base-structured-request/1".into(),
                    generation: input.generation.clone(),
                    max_retries: 1,
                },
                self.transport,
                self.guard,
            )?;
            observation.execution = base.execution;
            observation.output = base.output;
            observation.failure = base.failure;
            observation.response = base.attempts.last().and_then(|a| a.result.clone());
        } else {
            let mut request = input.generation.clone();
            request.operation = "generate".into();
            match generate_provider(self.calls, &request, self.transport, self.guard, 1) {
                Ok(response) => {
                    let parsed = response
                        .output
                        .as_str()
                        .ok_or_else(|| oe_semantic::providers::failure("empty_response", "parse"))
                        .and_then(oe_semantic::providers::parse_structured_json);
                    match parsed {
                        Ok(value) => {
                            let mut decoded = response.clone();
                            decoded.output = value.clone();
                            let checked = self.guard.check_result(&decoded);
                            match checked {
                                Ok(guard)
                                    if guard.schema == "semantica.provider-result-guard/1"
                                        && guard.credential_material_absent =>
                                {
                                    observation.execution = "completed".into();
                                    observation.output = value;
                                    observation.response = Some(response);
                                }
                                _ => {
                                    observation.failure = Some(oe_semantic::providers::failure(
                                        "credential_material_in_response",
                                        "guard",
                                    ))
                                }
                            }
                        }
                        Err(error) => {
                            observation.response = Some(response);
                            observation.failure = Some(error);
                        }
                    }
                }
                Err(error) => observation.failure = Some(error),
            }
        }
        observation.reserved = self.calls.snapshot().attempts > before;
        Ok(observation)
    }
}
