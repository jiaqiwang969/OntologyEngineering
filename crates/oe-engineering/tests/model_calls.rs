use oe_contracts::{ProviderTraceContext, SharedModelCallPolicy};
use oe_engineering::harness::{SharedModelCallContext, generate_provider, provider_trace};
use semantica_contracts::*;
use serde_json::{Value, json};
use std::cell::Cell;
fn request() -> SemanticProviderRequest {
    serde_json::from_value(json!({"schema":"semantica.provider-request/1","provider":"openai","interface_profile":"semantic_extract","operation":"generate","model":null,"custom_endpoint":false,"prompt":"Synthetic protocol.","options":{}})).unwrap()
}
struct ProviderFixture {
    calls: Cell<usize>,
    unknown: bool,
    failed: bool,
}
impl SemanticProviderTransportPort for ProviderFixture {
    fn post(
        &self,
        _: &SemanticProviderWireRequest,
    ) -> Result<SemanticProviderWireResponse, SemanticProviderFailure> {
        self.calls.set(self.calls.get() + 1);
        if self.failed {
            return Err(oe_semantic::providers::failure(
                "synthetic_failure",
                "transport",
            ));
        }
        let mut response =
            json!({"model":"resolved-synthetic","choices":[{"message":{"content":"answer"}}]});
        if !self.unknown {
            response["usage"] = json!({"prompt_tokens":2,"completion_tokens":3,"total_tokens":5});
        }
        let body = response.to_string();
        Ok(SemanticProviderWireResponse {
            http_status: 200,
            body_sha256: oe_core::sha256(body.as_bytes()),
            body,
            transport_kind: "injected_fixture".into(),
            transport_identity: "provider-tests/1".into(),
        })
    }
}
impl SemanticProviderResultGuardPort for ProviderFixture {
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
fn invalid_preflight_zero_budget_and_failed_provider_never_reset_shared_calls() {
    let calls = SharedModelCallContext::new(SharedModelCallPolicy { max_calls: 1 }).unwrap();
    let fixture = ProviderFixture {
        calls: Cell::new(0),
        unknown: false,
        failed: true,
    };
    let mut invalid = request();
    invalid.options = json!([]);
    assert!(generate_provider(&calls, &invalid, &fixture, &fixture, 1).is_err());
    assert_eq!(calls.snapshot().attempts, 0);
    assert_eq!(fixture.calls.get(), 0);
    assert!(generate_provider(&calls, &request(), &fixture, &fixture, 1).is_err());
    assert_eq!(calls.snapshot().attempts, 1);
    assert_eq!(calls.snapshot().unreported_attempts, 1);
    assert_eq!(
        generate_provider(&calls, &request(), &fixture, &fixture, 1)
            .unwrap_err()
            .code,
        "shared_model_call_budget_exhausted"
    );
    assert_eq!(fixture.calls.get(), 1);
    let zero = SharedModelCallContext::new(SharedModelCallPolicy { max_calls: 0 }).unwrap();
    assert_eq!(
        generate_provider(&zero, &request(), &fixture, &fixture, 1)
            .unwrap_err()
            .code,
        "shared_model_call_budget_exhausted"
    );
    assert_eq!(fixture.calls.get(), 1);
}
#[test]
fn same_dataset_provenance_binds_call_bytes_activity_and_no_adoption() {
    let calls = SharedModelCallContext::new(SharedModelCallPolicy { max_calls: 1 }).unwrap();
    let fixture = ProviderFixture {
        calls: Cell::new(0),
        unknown: false,
        failed: false,
    };
    let input = request();
    let result = generate_provider(&calls, &input, &fixture, &fixture, 1).unwrap();
    let context = ProviderTraceContext {
        graph_id: "urn:synthetic:model:graph".into(),
        activity_id: "urn:synthetic:model:activity".into(),
        agent_id: "urn:synthetic:model:agent".into(),
        record_id: "urn:synthetic:model:record".into(),
        output_id: "urn:synthetic:model:output".into(),
        started_at: Some("2026-10-02T00:00:00Z".into()),
        ended_at: Some("2026-10-02T00:00:01Z".into()),
    };
    let mut session = oe_semantic::providers::DatasetSession::new();
    let mutation = provider_trace(&mut session, &input, &result, &context, &fixture).unwrap();
    assert_eq!(mutation.revision, 1);
    let snapshot = session.snapshot().unwrap();
    assert!(snapshot.canonical_nquads.contains("SoftwareAgent"));
    assert!(snapshot.canonical_nquads.contains("wasGeneratedBy"));
    assert!(
        snapshot
            .canonical_nquads
            .contains("urn:synthetic:model:activity")
    );
    assert!(snapshot.canonical_nquads.contains("false"));
    assert!(snapshot.canonical_nquads.contains("not_determined"));
    assert!(
        snapshot
            .canonical_nquads
            .contains(&result.wire_request_sha256)
    );
    let mut changed = input.clone();
    changed.prompt = "Changed bytes".into();
    assert!(provider_trace(&mut session, &changed, &result, &context, &fixture).is_err());
    assert_eq!(session.snapshot().unwrap(), snapshot);
}

struct TransientProvider {
    calls: Cell<usize>,
}
impl SemanticProviderTransportPort for TransientProvider {
    fn post(
        &self,
        input: &SemanticProviderWireRequest,
    ) -> Result<SemanticProviderWireResponse, SemanticProviderFailure> {
        self.calls.set(self.calls.get() + 1);
        if self.calls.get() == 1 {
            let mut failure = oe_semantic::providers::failure("synthetic_rate_limit", "transport");
            failure.retryable = true;
            failure.retry_after = Some(0.0);
            failure.http_status = Some(429);
            return Err(failure);
        }
        ProviderFixture {
            calls: Cell::new(0),
            unknown: false,
            failed: false,
        }
        .post(input)
    }
}
impl SemanticProviderResultGuardPort for TransientProvider {
    fn check_result(
        &self,
        result: &SemanticProviderResult,
    ) -> Result<SemanticProviderResultGuard, SemanticProviderFailure> {
        ProviderFixture {
            calls: Cell::new(0),
            unknown: false,
            failed: false,
        }
        .check_result(result)
    }
}
#[test]
fn retry_is_caller_owned_and_each_attempt_consumes_same_shared_budget() {
    let context = SharedModelCallContext::new(SharedModelCallPolicy { max_calls: 2 }).unwrap();
    let transport = TransientProvider {
        calls: Cell::new(0),
    };
    generate_provider(&context, &request(), &transport, &transport, 3).unwrap();
    assert_eq!(transport.calls.get(), 2);
    assert_eq!(context.snapshot().attempts, 2);
    assert_eq!(context.snapshot().known_input_tokens, "2");
    assert_eq!(context.snapshot().known_output_tokens, "3");
    assert_eq!(context.snapshot().unreported_attempts, 1);
    let context = SharedModelCallContext::new(SharedModelCallPolicy { max_calls: 1 }).unwrap();
    let transport = TransientProvider {
        calls: Cell::new(0),
    };
    assert_eq!(
        generate_provider(&context, &request(), &transport, &transport, 3)
            .unwrap_err()
            .code,
        "shared_model_call_budget_exhausted"
    );
    assert_eq!(transport.calls.get(), 1);
    assert_eq!(context.snapshot().attempts, 1);
    assert_eq!(context.snapshot().unreported_attempts, 1);
}

#[test]
fn shared_context_counts_provider_failures_unknown_and_large_usage() {
    let calls = SharedModelCallContext::new(SharedModelCallPolicy { max_calls: 4 }).unwrap();
    struct LargeUsage;
    impl SemanticProviderTransportPort for LargeUsage {
        fn post(
            &self,
            _: &SemanticProviderWireRequest,
        ) -> Result<SemanticProviderWireResponse, SemanticProviderFailure> {
            let body = r#"{"choices":[{"message":{"content":"large usage fixture"}}],"usage":{"prompt_tokens":340282366920938463463374607431768211456,"completion_tokens":1}}"#.to_owned();
            Ok(SemanticProviderWireResponse {
                http_status: 200,
                body_sha256: oe_core::sha256(body.as_bytes()),
                body,
                transport_kind: "injected_fixture".into(),
                transport_identity: "large-usage-fixture/1".into(),
            })
        }
    }
    let provider = ProviderFixture {
        calls: Cell::new(0),
        unknown: false,
        failed: false,
    };
    generate_provider(&calls, &request(), &LargeUsage, &provider, 1).unwrap();
    generate_provider(&calls, &request(), &provider, &provider, 1).unwrap();
    let failed = ProviderFixture {
        unknown: true,
        calls: Cell::new(0),
        failed: true,
    };
    assert!(generate_provider(&calls, &request(), &failed, &failed, 1).is_err());
    let unknown = ProviderFixture {
        calls: Cell::new(0),
        unknown: true,
        failed: false,
    };
    let result = generate_provider(&calls, &request(), &unknown, &unknown, 1).unwrap();
    assert_eq!(result.usage_status, "unknown");
    let state = calls.snapshot();
    assert_eq!(state.attempts, 4);
    assert_eq!(
        state.known_input_tokens,
        "340282366920938463463374607431768211458"
    );
    assert_eq!(state.known_output_tokens, "4");
    assert_eq!(state.unreported_attempts, 2);
    assert_eq!(
        generate_provider(&calls, &request(), &provider, &provider, 3)
            .unwrap_err()
            .code,
        "shared_model_call_budget_exhausted"
    );
    assert_eq!(provider.calls.get(), 1);
    assert_eq!(unknown.calls.get(), 1);
    assert_eq!(failed.calls.get(), 1);
    assert_eq!(calls.snapshot(), state);
}
