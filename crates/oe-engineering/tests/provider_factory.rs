use oe_contracts::{ProviderTraceContext, SharedModelCallPolicy};
use oe_engineering::harness::*;
use semantica_contracts::*;
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
fn factory(operation: &str, name: Value, kwargs: Value) -> SemanticProviderFactoryRequest {
    SemanticProviderFactoryRequest {
        schema: "semantica.provider-factory-request/1".into(),
        operation: operation.into(),
        name,
        kwargs,
        kwargs_profile: "json/1".into(),
        constructor_id: None,
        use_pool: true,
    }
}
fn request() -> SemanticProviderRequest {
    serde_json::from_value(json!({"schema":"semantica.provider-request/1","provider":"openai","interface_profile":"semantic_extract",
        "operation":"generate","model":null,"custom_endpoint":false,"prompt":"frozen prompt","options":{}})).unwrap()
}
fn lite() -> SemanticLiteLlmRequest {
    let mut generation = request();
    generation.provider = "litellm".into();
    generation.interface_profile = "llms".into();
    generation.operation = "generate_structured".into();
    generation.model = Some("openai/original-model".into());
    SemanticLiteLlmRequest {
        generation,
        config: json!({"temperature":0.8}),
    }
}
struct Fixture {
    requests: RefCell<Vec<SemanticProviderWireRequest>>,
    failed: Cell<bool>,
}
impl SemanticProviderTransportPort for Fixture {
    fn post(
        &self,
        input: &SemanticProviderWireRequest,
    ) -> Result<SemanticProviderWireResponse, SemanticProviderFailure> {
        self.requests.borrow_mut().push(input.clone());
        if self.failed.get() {
            return Err(oe_semantic::providers::failure(
                "synthetic_dispatch_failed",
                "transport",
            ));
        }
        let body =
            json!({"model":"resolved-model","choices":[{"message":{"content":"{\"x\":1}"}}]})
                .to_string();
        Ok(SemanticProviderWireResponse {
            http_status: 200,
            body_sha256: oe_core::sha256(body.as_bytes()),
            body,
            transport_kind: "injected_fixture".into(),
            transport_identity: "factory-harness-fixture/1".into(),
        })
    }
}
impl SemanticProviderResultGuardPort for Fixture {
    fn check_result(
        &self,
        _: &SemanticProviderResult,
    ) -> Result<SemanticProviderResultGuard, SemanticProviderFailure> {
        Ok(SemanticProviderResultGuard {
            schema: "semantica.provider-result-guard/1".into(),
            credential_material_absent: true,
        })
    }
}

#[test]
fn unique_harness_registry_clear_preserves_budget_and_selected_backend_executes() {
    let calls = SharedModelCallContext::new(SharedModelCallPolicy { max_calls: 2 }).unwrap();
    let ctor = RustConfiguredProviderConstructors;
    let selected = calls
        .factory_apply(
            &factory(
                "create_provider",
                json!("GROQ"),
                json!({"model":"selected-model"}),
            ),
            &ctor,
        )
        .unwrap();
    let instance = selected.instance.unwrap();
    assert_eq!(instance.backend, "groq");
    assert_eq!(instance.default_model.as_deref(), Some("selected-model"));
    assert_eq!(instance.available, None);
    let f = Fixture {
        requests: RefCell::new(vec![]),
        failed: Cell::new(false),
    };
    let result = generate_factory_provider(&calls, &instance, &request(), &f, &f).unwrap();
    assert_eq!(f.requests.borrow()[0].provider, "groq");
    assert_eq!(f.requests.borrow()[0].body["model"], "selected-model");
    assert_eq!(result.provider, "groq");
    calls
        .factory_apply(&factory("clear", Value::Null, json!({})), &ctor)
        .unwrap();
    assert_eq!(calls.snapshot().attempts, 1);
    let newer = calls
        .factory_apply(
            &factory(
                "create_provider",
                json!("GROQ"),
                json!({"model":"selected-model"}),
            ),
            &ctor,
        )
        .unwrap()
        .instance
        .unwrap();
    assert_ne!(newer.instance_id, instance.instance_id);
    f.failed.set(true);
    assert!(generate_factory_provider(&calls, &newer, &request(), &f, &f).is_err());
    assert_eq!(calls.snapshot().attempts, 2);
    assert_eq!(calls.snapshot().unreported_attempts, 2);
    assert_eq!(
        generate_factory_provider(&calls, &newer, &request(), &f, &f)
            .unwrap_err()
            .code,
        "shared_model_call_budget_exhausted"
    );
    assert_eq!(f.requests.borrow().len(), 2);
}

#[test]
fn duplicate_options_and_unknown_sdk_adapter_never_reserve_but_failure_consumes_attempt() {
    let calls = SharedModelCallContext::new(SharedModelCallPolicy { max_calls: 1 }).unwrap();
    let f = Fixture {
        requests: RefCell::new(vec![]),
        failed: Cell::new(true),
    };
    let mut request = lite();
    request.generation.options = json!({"model":"collision"});
    assert_eq!(
        generate_litellm(&calls, &request, &f, &f).unwrap_err().code,
        "litellm_duplicate_model"
    );
    request.generation.options = json!({});
    request.generation.model = Some("anthropic/model".into());
    assert_eq!(
        generate_litellm(&calls, &request, &f, &f).unwrap_err().code,
        "litellm_sdk_provider_uncovered"
    );
    assert_eq!(calls.snapshot().attempts, 0);
    assert_eq!(f.requests.borrow().len(), 0);
    request.generation.model = Some("openai/model".into());
    assert!(generate_litellm(&calls, &request, &f, &f).is_err());
    assert_eq!(calls.snapshot().attempts, 1);
    assert_eq!(calls.snapshot().unreported_attempts, 1);
    assert_eq!(
        generate_litellm(&calls, &request, &f, &f).unwrap_err().code,
        "shared_model_call_budget_exhausted"
    );
    assert_eq!(f.requests.borrow().len(), 1);
}
#[test]
fn null_constructor_model_keeps_original_null_and_explicit_call_override() {
    let calls = SharedModelCallContext::new(SharedModelCallPolicy { max_calls: 0 }).unwrap();
    let instance = calls
        .factory_apply(
            &factory("create_provider", json!("openai"), json!({"model":null})),
            &RustConfiguredProviderConstructors,
        )
        .unwrap()
        .instance
        .unwrap();
    assert!(instance.default_model.is_none());
    let resolved = factory_provider_request(&instance, &request()).unwrap();
    assert!(
        oe_semantic::providers::prepare_request(&resolved)
            .unwrap()
            .body["model"]
            .is_null()
    );
    let mut explicit = request();
    explicit.options = json!({"model":"call-override"});
    let resolved = factory_provider_request(&instance, &explicit).unwrap();
    assert_eq!(
        oe_semantic::providers::prepare_request(&resolved)
            .unwrap()
            .body["model"],
        "call-override"
    );
}

#[test]
fn ordinary_provider_trace_retains_original_metadata_without_completion_boundary() {
    let calls = SharedModelCallContext::new(SharedModelCallPolicy { max_calls: 1 }).unwrap();
    let fixture = Fixture {
        requests: RefCell::new(vec![]),
        failed: Cell::new(false),
    };
    let input = request();
    let result = generate_provider(&calls, &input, &fixture, &fixture, 1).unwrap();
    let context = ProviderTraceContext {
        graph_id: "urn:original:graph".into(),
        agent_id: "urn:original:agent".into(),
        record_id: "urn:original:record".into(),
        output_id: "urn:original:output".into(),
        activity_id: "urn:original:activity".into(),
        started_at: None,
        ended_at: None,
    };
    let mut session = oe_semantic::providers::DatasetSession::new();
    provider_trace(&mut session, &input, &result, &context, &fixture).unwrap();
    let snapshot = session.snapshot().unwrap();
    assert!(
        !snapshot
            .canonical_nquads
            .contains("completion_request_sha256"),
        "Original ordinary provider has no completion boundary; null changes provenance/source identity"
    );
    assert!(
        snapshot
            .canonical_nquads
            .contains(&result.wire_request_sha256)
    );
}

#[test]
fn same_context_chat_litellm_factory_and_dataset_retain_unknown_and_reject_mismatched_input() {
    let calls = SharedModelCallContext::new(SharedModelCallPolicy { max_calls: 2 }).unwrap();
    let initial = Fixture {
        requests: RefCell::new(vec![]),
        failed: Cell::new(false),
    };
    generate_provider(&calls, &request(), &initial, &initial, 1).unwrap();
    let f = Fixture {
        requests: RefCell::new(vec![]),
        failed: Cell::new(false),
    };
    let request = lite();
    let result = generate_litellm(&calls, &request, &f, &f).unwrap();
    assert_eq!(result.output, json!({"x":1}));
    assert_eq!(result.provider, "openai");
    assert_eq!(
        result.requested_model.as_deref(),
        Some("openai/original-model")
    );
    assert_eq!(result.usage_status, "unknown");
    assert!(result.usage.input_tokens.is_none());
    let wire = &f.requests.borrow()[0];
    assert_eq!(result.wire_request_sha256, wire.body_sha256);
    assert_eq!(
        wire.body["messages"][0]["content"],
        "frozen prompt\n\nReturn the response as valid JSON only."
    );
    assert!(wire.body.get("response_format").is_none());
    let state = calls.snapshot();
    assert_eq!(state.attempts, 2);
    assert_eq!(state.known_input_tokens, "0");
    assert_eq!(state.known_output_tokens, "0");
    assert_eq!(state.unreported_attempts, 2);
    let context = ProviderTraceContext {
        graph_id: "urn:factory:graph".into(),
        agent_id: "urn:factory:agent".into(),
        record_id: "urn:factory:record".into(),
        output_id: "urn:factory:output".into(),
        activity_id: "urn:factory:activity".into(),
        started_at: None,
        ended_at: None,
    };
    let mut session = oe_semantic::providers::DatasetSession::new();
    litellm_trace(&mut session, &request, &result, &context, &f).unwrap();
    let snapshot = session.snapshot().unwrap();
    let completion = oe_semantic::providers::litellm::prepare_completion_request(&request).unwrap();
    assert!(snapshot.canonical_nquads.contains(&wire.body_sha256));
    assert!(snapshot.canonical_nquads.contains(&completion.body_sha256));
    assert!(snapshot.canonical_nquads.contains("not_determined"));
    let mut changed = request.clone();
    changed.generation.prompt = "changed".into();
    assert!(litellm_trace(&mut session, &changed, &result, &context, &f).is_err());
    assert_eq!(snapshot, session.snapshot().unwrap());
    calls
        .factory_apply(
            &factory("clear", Value::Null, json!({})),
            &RustConfiguredProviderConstructors,
        )
        .unwrap();
    assert_eq!(calls.snapshot(), state);
    assert_eq!(
        generate_litellm(&calls, &request, &f, &f).unwrap_err().code,
        "shared_model_call_budget_exhausted"
    );
    assert_eq!(f.requests.borrow().len(), 1);
    assert_eq!(initial.requests.borrow().len(), 1);
}
