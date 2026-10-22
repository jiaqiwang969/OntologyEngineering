//! Sole shared model-call authority, owned by the engineering Harness.
use num_bigint::BigUint;
use oe_contracts::{
    BaseStructuredAttempt, BaseStructuredFailure, BaseStructuredGenerationPort,
    BaseStructuredRequest, BaseStructuredResult, ProviderTraceContext, SharedModelCallPolicy,
    SharedModelCallState,
};
use semantica_contracts::*;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    str::FromStr,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

static NEXT: AtomicU64 = AtomicU64::new(1);
struct State {
    attempts: u64,
    input: BigUint,
    output: BigUint,
    unreported: u64,
}
pub struct SharedModelCallContext {
    id: u64,
    policy: SharedModelCallPolicy,
    state: RefCell<State>,
    provider_factory: RefCell<SemanticProviderFactoryState>,
}
struct Reservation {
    owner: u64,
}
impl SharedModelCallContext {
    pub fn new(policy: SharedModelCallPolicy) -> Result<Self, SemanticProviderFailure> {
        if policy.max_calls > 4096 {
            return Err(oe_semantic::providers::failure(
                "shared_model_call_policy_invalid",
                "budget",
            ));
        }
        Ok(Self {
            id: NEXT.fetch_add(1, Ordering::Relaxed),
            policy,
            state: RefCell::new(State {
                attempts: 0,
                input: 0u8.into(),
                output: 0u8.into(),
                unreported: 0,
            }),
            provider_factory: RefCell::new(oe_semantic::providers::factory::empty_state()),
        })
    }
    fn reserve(&self) -> Result<Reservation, SemanticProviderFailure> {
        let mut state = self.state.borrow_mut();
        if state.attempts >= self.policy.max_calls {
            return Err(oe_semantic::providers::failure(
                "shared_model_call_budget_exhausted",
                "budget",
            ));
        }
        state.attempts += 1;
        state.unreported += 1;
        Ok(Reservation { owner: self.id })
    }
    fn observe(&self, reservation: Reservation, input: Option<String>, output: Option<String>) {
        assert_eq!(
            reservation.owner, self.id,
            "private_reservation_context_mismatch"
        );
        let parse = |text: Option<String>| {
            text.filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
                .and_then(|s| BigUint::from_str(&s).ok())
        };
        let input = parse(input);
        let output = parse(output);
        let mut state = self.state.borrow_mut();
        if input.is_some() && output.is_some() {
            state.unreported -= 1;
        }
        if let Some(input) = input {
            state.input += input;
        }
        if let Some(output) = output {
            state.output += output;
        }
    }
    pub fn snapshot(&self) -> SharedModelCallState {
        let state = self.state.borrow();
        SharedModelCallState {
            max_calls: self.policy.max_calls,
            attempts: state.attempts,
            known_input_tokens: state.input.to_string(),
            known_output_tokens: state.output.to_string(),
            unreported_attempts: state.unreported,
        }
    }
    /// Registry and instance reuse share this caller's lifetime; clearing them
    /// cannot reset reservations, credentials or DatasetSession provenance.
    pub fn factory_apply(
        &self,
        request: &SemanticProviderFactoryRequest,
        constructors: &dyn SemanticProviderConstructionPort,
    ) -> Result<SemanticProviderFactoryResult, SemanticProviderFactoryFailure> {
        oe_semantic::providers::factory::RustProviderFactory::new(
            &self.provider_factory,
            constructors,
        )
        .apply(request)
    }
}

/// Configure original built-in constructor bindings without guessing SDK
/// availability. Actual SDK initialization/remote inference is a separate port.
/// Callers requiring custom providers inject their own constructor through the
/// same factory API; this adapter never creates a global custom registry.
pub struct RustConfiguredProviderConstructors;
impl SemanticProviderConstructionPort for RustConfiguredProviderConstructors {
    fn construct(
        &self,
        input: &SemanticProviderConstructionRequest,
    ) -> Result<SemanticProviderInstance, SemanticProviderFactoryFailure> {
        let fail = oe_semantic::providers::factory::failure;
        if input.validate().is_err() || input.backend == "custom" {
            return Err(fail(
                "factory_constructor_unavailable",
                "construct",
                "LookupError",
            ));
        }
        let default = oe_semantic::providers::factory::builtin_default_model(&input.backend)
            .ok_or_else(|| fail("factory_unknown_provider", "construct", "ValueError"))?;
        let field = if input.backend == "huggingface_llm" {
            "model_name"
        } else {
            "model"
        };
        let supplied = input.kwargs.get(field);
        let model = match (supplied, input.kwargs_profile.as_str()) {
            (None, _) => Some(default.to_owned()),
            (Some(value), "json/1") => match value {
                Value::Null => None,
                Value::String(value) => Some(value.clone()),
                _ => {
                    return Err(fail(
                        "factory_nonstring_model_uncovered",
                        "construct",
                        "TypeError",
                    ));
                }
            },
            (Some(value), "python-values/1") if value["kind"] == "scalar" => {
                match &value["value"] {
                    Value::Null => None,
                    Value::String(value) => Some(value.clone()),
                    _ => {
                        return Err(fail(
                            "factory_nonstring_model_uncovered",
                            "construct",
                            "TypeError",
                        ));
                    }
                }
            }
            _ => {
                return Err(fail(
                    "factory_constructor_profile_uncovered",
                    "construct",
                    "TypeError",
                ));
            }
        };
        Ok(SemanticProviderInstance {
            instance_id: input.instance_id.clone(),
            backend: input.backend.clone(),
            constructor_id: input.constructor_id.clone(),
            provider_name: input.name.clone(),
            default_model: model,
            available: None,
            configuration_sha256: input.configuration_sha256.clone(),
        })
    }
}

/// Use the selected constructor's actual backend/default through the existing
/// transport and one call owner. Unsupported execution remains explicit.
pub fn generate_factory_provider(
    calls: &SharedModelCallContext,
    instance: &SemanticProviderInstance,
    request: &SemanticProviderRequest,
    transport: &dyn SemanticProviderTransportPort,
    guard: &dyn SemanticProviderResultGuardPort,
) -> Result<SemanticProviderResult, SemanticProviderFailure> {
    let request = factory_provider_request(instance, request)?;
    generate_provider(calls, &request, transport, guard, 1)
}

pub fn factory_provider_request(
    instance: &SemanticProviderInstance,
    request: &SemanticProviderRequest,
) -> Result<SemanticProviderRequest, SemanticProviderFailure> {
    if instance.validate().is_err() || !matches!(instance.backend.as_str(), "openai" | "groq") {
        return Err(oe_semantic::providers::failure(
            "factory_execution_backend_uncovered",
            "validate",
        ));
    }
    if instance.available == Some(false) {
        return Err(oe_semantic::providers::failure(
            "factory_provider_unavailable",
            "validate",
        ));
    }
    let mut request = request.clone();
    request.provider = instance.backend.clone();
    request.interface_profile = "semantic_extract".into();
    request.model = instance.default_model.clone();
    if instance.default_model.is_none()
        && request.options.is_object()
        && request.options.get("model").is_none()
    {
        request.options["model"] = Value::Null;
    }
    oe_semantic::providers::prepare_request(&request)?;
    Ok(request)
}

/// One original LiteLLM completion boundary; no SDK-owned retry/fallback,
/// parallel budget or credential lookup is activated.
pub fn generate_litellm(
    calls: &SharedModelCallContext,
    request: &SemanticLiteLlmRequest,
    transport: &dyn SemanticProviderTransportPort,
    guard: &dyn SemanticProviderResultGuardPort,
) -> Result<SemanticProviderResult, SemanticProviderFailure> {
    let completion = oe_semantic::providers::litellm::prepare_completion_request(request)
        .map_err(|e| oe_semantic::providers::failure(&e.code, &e.phase))?;
    let protocol = oe_semantic::providers::litellm::prepare_protocol_request(request, &completion)?;
    let transport = SharedProviderTransport {
        inner: transport,
        calls,
    };
    let response = transport.post(&protocol)?;
    let result = oe_semantic::providers::litellm::decode_protocol_response(
        request,
        &completion,
        &protocol,
        &response,
    )?;
    let secrecy = guard.check_result(&result)?;
    if secrecy.schema != "semantica.provider-result-guard/1" || !secrecy.credential_material_absent
    {
        return Err(oe_semantic::providers::failure(
            "credential_material_in_response",
            "guard",
        ));
    }
    Ok(result)
}

struct SharedProviderTransport<'a> {
    inner: &'a dyn SemanticProviderTransportPort,
    calls: &'a SharedModelCallContext,
}
impl SemanticProviderTransportPort for SharedProviderTransport<'_> {
    fn post(
        &self,
        input: &SemanticProviderWireRequest,
    ) -> Result<SemanticProviderWireResponse, SemanticProviderFailure> {
        let reservation = self.calls.reserve()?;
        let response = self.inner.post(input)?;
        let usage = oe_semantic::providers::parse_protocol_json(&response.body)
            .ok()
            .map(|v| oe_semantic::providers::response_usage(&v).0);
        self.calls.observe(
            reservation,
            usage.as_ref().and_then(|v| v.input_tokens.clone()),
            usage.and_then(|v| v.output_tokens),
        );
        Ok(response)
    }
}
/// Retries use the same shared context; all real dispatches reserve before send.
pub fn generate_provider(
    calls: &SharedModelCallContext,
    request: &SemanticProviderRequest,
    transport: &dyn SemanticProviderTransportPort,
    guard: &dyn SemanticProviderResultGuardPort,
    max_attempts: usize,
) -> Result<SemanticProviderResult, SemanticProviderFailure> {
    if !(1..=3).contains(&max_attempts) {
        return Err(oe_semantic::providers::failure(
            "invalid_attempt_limit",
            "budget",
        ));
    }
    oe_semantic::providers::prepare_request(request)?;
    let transport = SharedProviderTransport {
        inner: transport,
        calls,
    };
    let runtime = oe_semantic::providers::RustProviderRuntime::new(&transport, guard);
    for index in 0..max_attempts {
        match runtime.generate(request) {
            Ok(result) => return Ok(result),
            Err(error) => {
                let delay = error.retry_after.unwrap_or(0.5 * 2_f64.powi(index as i32));
                if index + 1 == max_attempts
                    || !error.retryable
                    || !delay.is_finite()
                    || !(0.0..=5.0).contains(&delay)
                {
                    return Err(error);
                }
                std::thread::sleep(Duration::from_secs_f64(delay));
            }
        }
    }
    unreachable!("nonempty attempt limit")
}

/// Original BaseProvider retry loop, owned by the same Harness call authority.
pub struct RustBaseStructuredHarness<'a> {
    calls: &'a SharedModelCallContext,
    transport: &'a dyn SemanticProviderTransportPort,
    guard: &'a dyn SemanticProviderResultGuardPort,
}
impl<'a> RustBaseStructuredHarness<'a> {
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
impl BaseStructuredGenerationPort for RustBaseStructuredHarness<'_> {
    fn generate_structured(
        &self,
        input: &BaseStructuredRequest,
    ) -> Result<BaseStructuredResult, BaseStructuredFailure> {
        generate_base_structured(self.calls, input, self.transport, self.guard)
            .map_err(|failure| BaseStructuredFailure { failure })
    }
}

pub fn generate_base_structured(
    calls: &SharedModelCallContext,
    input: &BaseStructuredRequest,
    transport: &dyn SemanticProviderTransportPort,
    guard: &dyn SemanticProviderResultGuardPort,
) -> Result<BaseStructuredResult, SemanticProviderFailure> {
    generate_base_structured_with_delay(calls, input, transport, guard, std::thread::sleep)
}

pub fn generate_base_structured_with_delay<F: FnMut(Duration)>(
    calls: &SharedModelCallContext,
    input: &BaseStructuredRequest,
    transport: &dyn SemanticProviderTransportPort,
    guard: &dyn SemanticProviderResultGuardPort,
    mut delay: F,
) -> Result<BaseStructuredResult, SemanticProviderFailure> {
    if input.validate().is_err() || input.max_retries > 4096 {
        return Err(oe_semantic::providers::failure(
            "base_structured_request_invalid",
            "validate",
        ));
    }
    let logical = oe_core::sha256(serde_json::to_vec(input).map_err(|_| {
        oe_semantic::providers::failure("base_structured_request_invalid", "validate")
    })?);
    let mut outcome = BaseStructuredResult {
        schema: "oev2.base-structured-result/1".into(),
        execution: "completed".into(),
        logical_request_sha256: logical,
        output: json!([]),
        attempts: vec![],
        failure: None,
    };
    let mut request = input.generation.clone();
    request.operation = "generate".into();
    request.prompt = oe_semantic::providers::base_structured_prompt(&request.prompt);
    let max_retries = input.max_retries.max(0) as u64;
    for index in 0..max_retries {
        let before = calls.snapshot().attempts;
        let generation = generate_provider(calls, &request, transport, guard, 1);
        let mut attempt = BaseStructuredAttempt {
            iteration: index + 1,
            request: request.clone(),
            result: None,
            failure: None,
            reserved: calls.snapshot().attempts > before,
        };
        let parsed = match generation {
            Ok(result) => {
                let parsed = result
                    .output
                    .as_str()
                    .ok_or_else(|| oe_semantic::providers::failure("empty_response", "parse"))
                    .and_then(oe_semantic::providers::parse_structured_json);
                attempt.result = Some(result.clone());
                parsed.and_then(|value| {
                    // A second JSON decode can expose credential bytes that were
                    // escaped inside an otherwise safe plaintext response.
                    let mut checked = result;
                    checked.output = value.clone();
                    let secrecy = guard.check_result(&checked)?;
                    if secrecy.schema != "semantica.provider-result-guard/1"
                        || !secrecy.credential_material_absent
                    {
                        return Err(oe_semantic::providers::failure(
                            "credential_material_in_response",
                            "guard",
                        ));
                    }
                    Ok(value)
                })
            }
            Err(error) => Err(error),
        };
        match parsed {
            Ok(value) => {
                outcome.attempts.push(attempt);
                if !oe_semantic::providers::json_truthy(&value) && index + 1 < max_retries {
                    // Original empty results retry immediately, without sleep.
                    continue;
                }
                outcome.output = value;
                outcome.failure = None;
                return Ok(outcome);
            }
            Err(error) => {
                if error.phase == "guard" {
                    attempt.result = None;
                }
                attempt.failure = Some(error.clone());
                outcome.attempts.push(attempt);
                // Policy refusals are not model errors. The same owner must
                // stop before further dispatch, even in the legacy retry loop.
                let stop = matches!(
                    error.phase.as_str(),
                    "budget" | "guard" | "validate" | "configure"
                );
                outcome.failure = Some(error);
                if stop || index + 1 == max_retries {
                    outcome.execution = "failed".into();
                    outcome.output = Value::Null;
                    return Ok(outcome);
                }
                delay(Duration::from_secs((index + 1) * 2));
            }
        }
    }
    Ok(outcome)
}

/// One original OpenAIStore call; the same context reserves each transport send.
pub fn embed_provider(
    calls: &SharedModelCallContext,
    input: &SemanticEmbeddingRequest,
    transport: &dyn SemanticProviderTransportPort,
    guard: &dyn SemanticProviderResultGuardPort,
) -> Result<SemanticProviderResult, SemanticProviderFailure> {
    oe_semantic::providers::prepare_embedding_request(input)?;
    let shared = SharedProviderTransport {
        inner: transport,
        calls,
    };
    let runtime = oe_semantic::providers::RustProviderRuntime::new(&shared, guard);
    runtime.embed(input)
}

/// Native CPU and explicit HTTP providers share this context. No model or second ledger is owned here.
pub fn execute_native_model(
    calls: &SharedModelCallContext,
    input: &SemanticNativeRequest,
    runtime: &dyn SemanticNativeInferencePort,
    guard: &dyn SemanticNativeResultGuardPort,
) -> Result<SemanticNativeResult, SemanticProviderFailure> {
    oe_semantic::native_models::validate_native_request(input)?;
    let result = if input.operation == "embed" && input.batch && input.texts.is_empty() {
        runtime.execute(input)?
    } else {
        let reservation = calls.reserve()?;
        let result = runtime.execute(input);
        match &result {
            Ok(v) => calls.observe(
                reservation,
                v.usage.input_tokens.clone(),
                v.usage.output_tokens.clone(),
            ),
            Err(_) => calls.observe(reservation, None, None),
        }
        result?
    };
    let checked = guard.check_native_result(&result)?;
    if checked.schema != "semantica.provider-result-guard/1" || !checked.credential_material_absent
    {
        return Err(oe_semantic::providers::failure(
            "credential_material_in_response",
            "guard",
        ));
    }
    if result.validate().is_err()
        || result.request_sha256
            != oe_core::sha256(serde_json::to_vec(input).map_err(|_| {
                oe_semantic::providers::failure("native_request_invalid", "validate")
            })?)
        || result.model_sha256
            != oe_core::sha256(serde_json::to_vec(&input.model).map_err(|_| {
                oe_semantic::providers::failure("native_request_invalid", "validate")
            })?)
    {
        return Err(oe_semantic::providers::failure(
            "native_result_input_mismatch",
            "guard",
        ));
    }
    Ok(result)
}

/// Preserve original sequential batch/first-failure behavior. Prior checked
/// observations survive, but failed batches expose no partial vector result.
pub fn embed_provider_batch(
    calls: &SharedModelCallContext,
    input: &SemanticEmbeddingBatchRequest,
    transport: &dyn SemanticProviderTransportPort,
    guard: &dyn SemanticProviderResultGuardPort,
) -> Result<SemanticEmbeddingBatchResult, SemanticProviderFailure> {
    if input.validate().is_err() || input.requests.len() > 4096 {
        return Err(oe_semantic::providers::failure(
            "embedding_batch_request_invalid",
            "validate",
        ));
    }
    let raw = serde_json::to_vec(input).map_err(|_| {
        oe_semantic::providers::failure("embedding_batch_request_invalid", "validate")
    })?;
    let mut batch = SemanticEmbeddingBatchResult {
        schema: "semantica.embedding-batch-result/1".into(),
        execution: "completed".into(),
        logical_request_sha256: oe_core::sha256(raw),
        results: vec![],
        output: None,
        failure: None,
        failed_index: None,
        attempts: vec![],
    };
    for (index, request) in input.requests.iter().enumerate() {
        let before = calls.snapshot().attempts;
        let mut attempt = SemanticEmbeddingAttempt {
            index: index as u64,
            request: request.clone(),
            result: None,
            failure: None,
            reserved: false,
        };
        match embed_provider(calls, request, transport, guard) {
            Ok(result) => {
                attempt.result = Some(result.clone());
                batch.results.push(result);
            }
            Err(error) => {
                attempt.reserved = calls.snapshot().attempts > before;
                attempt.failure = Some(error.clone());
                batch.attempts.push(attempt);
                batch.execution = "failed".into();
                batch.failure = Some(error);
                batch.failed_index = Some(index as u64);
                return Ok(batch);
            }
        }
        attempt.reserved = calls.snapshot().attempts > before;
        batch.attempts.push(attempt);
    }
    match oe_semantic::providers::stack_embedding_results(&batch.results) {
        Ok(output) => batch.output = Some(output),
        Err(error) => {
            batch.execution = "failed".into();
            batch.failure = Some(error);
        }
    }
    Ok(batch)
}

/// Add a model activity to the existing sole DatasetSession/provenance algorithm.
/// Only checked wire input and checked result bytes become source entities.
pub fn provider_trace(
    session: &mut oe_semantic::providers::DatasetSession,
    request: &SemanticProviderRequest,
    result: &SemanticProviderResult,
    context: &ProviderTraceContext,
    guard: &dyn SemanticProviderResultGuardPort,
) -> anyhow::Result<SemanticMutationResult> {
    let wire =
        oe_semantic::providers::prepare_request(request).map_err(|e| anyhow::anyhow!(e.code))?;
    trace_model_result(
        session,
        &oe_core::sha256(serde_json::to_vec(request)?),
        &wire,
        &request.prompt,
        "llm_generation",
        result,
        context,
        guard,
    )
}

pub fn embedding_trace(
    session: &mut oe_semantic::providers::DatasetSession,
    request: &SemanticEmbeddingRequest,
    result: &SemanticProviderResult,
    context: &ProviderTraceContext,
    guard: &dyn SemanticProviderResultGuardPort,
) -> anyhow::Result<SemanticMutationResult> {
    let wire = oe_semantic::providers::prepare_embedding_request(request)
        .map_err(|e| anyhow::anyhow!(e.code))?;
    trace_model_result(
        session,
        &oe_core::sha256(serde_json::to_vec(request)?),
        &wire,
        &request.input,
        "embedding_generation",
        result,
        context,
        guard,
    )
}

pub fn litellm_trace(
    session: &mut oe_semantic::providers::DatasetSession,
    request: &SemanticLiteLlmRequest,
    result: &SemanticProviderResult,
    context: &ProviderTraceContext,
    guard: &dyn SemanticProviderResultGuardPort,
) -> anyhow::Result<SemanticMutationResult> {
    let completion = oe_semantic::providers::litellm::prepare_completion_request(request)
        .map_err(|e| anyhow::anyhow!(e.code))?;
    let wire = oe_semantic::providers::litellm::prepare_protocol_request(request, &completion)
        .map_err(|e| anyhow::anyhow!(e.code))?;
    // The completion body is an independent source, while the existing trace
    // verifies and records the actual HTTP wire body.
    trace_model_result_with_completion(
        session,
        &oe_core::sha256(serde_json::to_vec(request)?),
        &wire,
        &request.generation.prompt,
        "litellm_generation",
        result,
        context,
        guard,
        Some(&completion),
    )
}

fn trace_model_result(
    session: &mut oe_semantic::providers::DatasetSession,
    request_sha256: &str,
    wire: &SemanticProviderWireRequest,
    preview: &str,
    source_kind: &str,
    result: &SemanticProviderResult,
    context: &ProviderTraceContext,
    guard: &dyn SemanticProviderResultGuardPort,
) -> anyhow::Result<SemanticMutationResult> {
    trace_model_result_with_completion(
        session,
        request_sha256,
        wire,
        preview,
        source_kind,
        result,
        context,
        guard,
        None,
    )
}
fn trace_model_result_with_completion(
    session: &mut oe_semantic::providers::DatasetSession,
    request_sha256: &str,
    wire: &SemanticProviderWireRequest,
    preview: &str,
    source_kind: &str,
    result: &SemanticProviderResult,
    context: &ProviderTraceContext,
    guard: &dyn SemanticProviderResultGuardPort,
    completion: Option<&SemanticProviderWireRequest>,
) -> anyhow::Result<SemanticMutationResult> {
    anyhow::ensure!(
        context.validate().is_ok() && result.validate().is_ok(),
        "provider_trace_contract_invalid"
    );
    let checked = guard
        .check_result(result)
        .map_err(|e| anyhow::anyhow!(e.code))?;
    anyhow::ensure!(
        checked.credential_material_absent && checked.schema == "semantica.provider-result-guard/1",
        "provider_trace_guard_failed"
    );
    anyhow::ensure!(
        request_sha256 == result.request_sha256
            && wire.body_sha256 == result.wire_request_sha256
            && wire.provider == result.provider
            && wire.operation == result.operation,
        "provider_trace_request_mismatch"
    );
    let mut metadata = json!({"source":source_kind,"model":result.resolved_model,
        "requested_model":result.requested_model,"prompt_preview":preview.chars().take(200).collect::<String>(),
        "response_preview":result.output.as_str().map(str::to_owned).unwrap_or_else(|| result.output.to_string()).chars().take(200).collect::<String>(),
        "token_usage":result.usage,"usage_status":result.usage_status,
        "request_sha256":result.request_sha256,"wire_request_sha256":result.wire_request_sha256,
        "response_sha256":result.response_sha256,"execution_kind":result.transport_kind,
        "transport_identity":result.transport_identity,"is_automated":true,
        "cost":{"status":"unknown","amount":null,"currency":null},
        "engineering_acceptance":"not_determined","fact_admission":false});
    if let Some(completion) = completion {
        metadata["completion_request_sha256"] = json!(completion.body_sha256);
    }
    let input = if let Some(completion) = completion {
        serde_json::to_string(&json!({"completion":completion,"actual_wire":wire}))?
    } else {
        serde_json::to_string(&wire.body)?
    };
    trace_model_sources(
        session,
        input,
        serde_json::to_string(result)?,
        metadata,
        context,
    )
}

pub fn native_model_trace(
    session: &mut oe_semantic::providers::DatasetSession,
    request: &SemanticNativeRequest,
    result: &SemanticNativeResult,
    context: &ProviderTraceContext,
    guard: &dyn SemanticNativeResultGuardPort,
) -> anyhow::Result<SemanticMutationResult> {
    anyhow::ensure!(
        context.validate().is_ok() && result.validate().is_ok(),
        "native_trace_contract_invalid"
    );
    let checked = guard
        .check_native_result(result)
        .map_err(|e| anyhow::anyhow!(e.code))?;
    anyhow::ensure!(
        checked.schema == "semantica.provider-result-guard/1" && checked.credential_material_absent,
        "native_trace_guard_failed"
    );
    anyhow::ensure!(
        oe_core::sha256(serde_json::to_vec(request)?) == result.request_sha256
            && oe_core::sha256(serde_json::to_vec(&request.model)?) == result.model_sha256,
        "native_trace_input_identity_mismatch"
    );
    let metadata = json!({"source":"native_model_inference","model":request.model.model_id,
        "revision":request.model.revision,"model_sha256":result.model_sha256,"request_sha256":result.request_sha256,
        "model_assets":request.model.assets,"native_profile":result.profile,"execution_identity":result.execution_identity,
        "parameters":result.parameters,"token_inputs":result.token_inputs,"token_usage":result.usage,
        "usage_status":result.usage_status,"is_automated":true,"cost":{"status":"unknown","amount":null,"currency":null},
        "engineering_acceptance":"not_determined","fact_admission":false});
    trace_model_sources(
        session,
        serde_json::to_string(request)?,
        serde_json::to_string(result)?,
        metadata,
        context,
    )
}
fn trace_model_sources(
    session: &mut oe_semantic::providers::DatasetSession,
    input: String,
    output: String,
    metadata: Value,
    context: &ProviderTraceContext,
) -> anyhow::Result<SemanticMutationResult> {
    let input_id = format!("{}:input", context.activity_id);
    let output_id = context.output_id.clone();
    let source = |id: String, data: String| SemanticProvenanceSource {
        binding: SemanticSourceBinding {
            schema: "semantica.source-binding/1".into(),
            source_id: id,
            source_sha256: oe_core::sha256(data.as_bytes()),
            byte_count: data.len() as u64,
        },
        data,
        spans: vec![],
    };
    let batch = SemanticProvenanceBatchRequest {
        schema: "semantica.provenance-batch/1".into(),
        graph_id: context.graph_id.clone(),
        max_records: 1,
        max_quads: 4096,
        sources: vec![
            source(input_id.clone(), input),
            source(output_id.clone(), output),
        ],
        agents: vec![SemanticProvenanceAgent {
            agent_id: context.agent_id.clone(),
            kind: "software_agent".into(),
        }],
        activities: vec![SemanticProvenanceActivity {
            activity_id: context.activity_id.clone(),
            started_at: context.started_at.clone(),
            ended_at: context.ended_at.clone(),
            used: vec![input_id.clone()],
            associations: vec![SemanticProvenanceAssociation {
                association_id: format!("{}:association", context.activity_id),
                agent_id: context.agent_id.clone(),
                role_id: None,
            }],
        }],
        records: vec![SemanticProvenanceRecord {
            record_id: context.record_id.clone(),
            subject_id: output_id,
            entity_kind: "entity".into(),
            attributed_to: vec![context.agent_id.clone()],
            generated_by: Some(context.activity_id.clone()),
            derived_from: vec![input_id],
            revision_of: vec![],
            source_spans: vec![],
            metadata: Some(metadata),
            confidence: None,
        }],
    };
    oe_semantic::providers::load_records(session, &batch)
}

pub fn provider_call_time() -> String {
    chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now()).to_rfc3339()
}
