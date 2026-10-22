use oe_contracts::{ProviderTraceContext, SharedModelCallPolicy};
use oe_engineering::harness::*;
use semantica_contracts::*;
use serde_json::{Value, json};
use std::cell::Cell;
fn request() -> SemanticNativeRequest {
    let gold: Value = serde_json::from_slice(include_bytes!(
        "../../semantica-runtime/tests/fixtures/native-cpu-oracles.json"
    ))
    .unwrap();
    serde_json::from_value(json!({"schema":"semantica.native-request/1","model":gold["model_specs"]["bge-small"],
        "operation":"embed","interface_profile":"text_embedder","batch":false,"texts":["synthetic owner input"],
        "normalize":true,"options":{},"seed":77})).unwrap()
}
struct Counter {
    dispatches: Cell<u64>,
    fail: bool,
}
impl SemanticNativeInferencePort for Counter {
    fn execute(
        &self,
        input: &SemanticNativeRequest,
    ) -> Result<SemanticNativeResult, SemanticProviderFailure> {
        self.dispatches.set(self.dispatches.get() + 1);
        if self.fail {
            return Err(oe_semantic::providers::failure(
                "synthetic_native_failure",
                "native",
            ));
        }
        Ok(SemanticNativeResult {
            schema: "semantica.native-result/1".into(),
            execution: "completed".into(),
            profile: "semantica.native-candle-f32-cpu/1".into(),
            request_sha256: oe_core::sha256(serde_json::to_vec(input).unwrap()),
            model_sha256: oe_core::sha256(serde_json::to_vec(&input.model).unwrap()),
            parameters: json!({"test_domain":"budget fixture, not weighted inference proof"}),
            token_inputs: json!({"input_ids":[[1,2,3,4]]}),
            output: json!({"dtype":"float32","shape":[1],"bits":[1065353216]}),
            usage: SemanticProviderUsage {
                input_tokens: Some("4".into()),
                output_tokens: None,
                total_tokens: None,
            },
            usage_status: "partial".into(),
            execution_identity: "injected-native-budget-fixture/1".into(),
        })
    }
}
struct Guard;
impl SemanticNativeResultGuardPort for Guard {
    fn check_native_result(
        &self,
        _: &SemanticNativeResult,
    ) -> Result<SemanticProviderResultGuard, SemanticProviderFailure> {
        Ok(SemanticProviderResultGuard {
            schema: "semantica.provider-result-guard/1".into(),
            credential_material_absent: true,
        })
    }
}

#[test]
fn native_trace_rejects_changed_input_and_uses_same_dataset() {
    let input = request();
    let port = Counter {
        dispatches: Cell::new(0),
        fail: false,
    };
    let result = port.execute(&input).unwrap();
    let mut session = oe_semantic::providers::DatasetSession::new();
    let context = ProviderTraceContext {
        graph_id: "urn:native:graph".into(),
        activity_id: "urn:native:activity".into(),
        agent_id: "urn:native:software".into(),
        record_id: "urn:native:record".into(),
        output_id: "urn:native:output".into(),
        started_at: None,
        ended_at: None,
    };
    native_model_trace(&mut session, &input, &result, &context, &Guard).unwrap();
    let before = session.snapshot().unwrap();
    assert!(before.canonical_nquads.contains("native_model_inference"));
    let mut changed = input.clone();
    changed.texts[0] = "changed source bytes".into();
    assert!(native_model_trace(&mut session, &changed, &result, &context, &Guard).is_err());
    assert_eq!(session.snapshot().unwrap(), before);
    let mut changed = input.clone();
    changed.model.revision = "changed weights baseline".into();
    assert!(native_model_trace(&mut session, &changed, &result, &context, &Guard).is_err());
    assert_eq!(session.snapshot().unwrap(), before);
}

#[test]
fn same_context_reserves_each_native_call_before_actual_dispatch() {
    let calls = SharedModelCallContext::new(SharedModelCallPolicy { max_calls: 2 }).unwrap();
    let port = Counter {
        dispatches: Cell::new(0),
        fail: false,
    };
    execute_native_model(&calls, &request(), &port, &Guard).unwrap();
    execute_native_model(&calls, &request(), &port, &Guard).unwrap();
    assert_eq!(port.dispatches.get(), 2);
    assert_eq!(calls.snapshot().attempts, 2);
    assert_eq!(calls.snapshot().known_input_tokens, "8");
    assert_eq!(calls.snapshot().known_output_tokens, "0");
    assert_eq!(calls.snapshot().unreported_attempts, 2);
    let error = execute_native_model(&calls, &request(), &port, &Guard).unwrap_err();
    assert_eq!(error.code, "shared_model_call_budget_exhausted");
    assert_eq!(port.dispatches.get(), 2);
    let failed = Counter {
        dispatches: Cell::new(0),
        fail: true,
    };
    let budget = SharedModelCallContext::new(SharedModelCallPolicy { max_calls: 1 }).unwrap();
    assert!(execute_native_model(&budget, &request(), &failed, &Guard).is_err());
    assert_eq!(budget.snapshot().attempts, 1);
    assert_eq!(budget.snapshot().unreported_attempts, 1);
    assert_eq!(failed.dispatches.get(), 1);
    assert_eq!(
        execute_native_model(&budget, &request(), &failed, &Guard)
            .unwrap_err()
            .phase,
        "budget"
    );
    assert_eq!(failed.dispatches.get(), 1);
}
