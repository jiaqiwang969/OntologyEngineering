use oe_contracts::SharedModelCallPolicy;
use oe_engineering::harness::{RustTypedDispatchHarness, SharedModelCallContext};
use oe_semantic::{schema::RustSchemaRuntime, typed::RustTypedRuntime};
use semantica_contracts::*;
use serde_json::{Value, json};
use std::{cell::RefCell, collections::VecDeque, time::Duration};

// These byte-pinned historical inputs include private source provenance and are
// supplied separately from the public distribution. Missing inputs fail openly.
fn original_input(relative: &str) -> std::path::PathBuf {
    let root = std::env::var_os("OE_TYPED_ORIGINAL_INPUT_ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../sources/private-regression/typed-generation")
        });
    let expected = match relative {
        "frozen-original-inputs/FROZEN-MANUAL-ORACLE-INPUTS.json" => {
            "41e51f8a5e65f99d397ca176183de64527f2ad7d5a8f0ed3c86a2a0a421b2a0b"
        }
        "frozen-original-inputs/ACTUAL-MANUAL-ORACLE.json" => {
            "045ff1e72cf738ed006fdca75ec69877fcde9adae9ac844292a32853eabde71e"
        }
        "extra-manual-numeric-01/FROZEN-MANUAL-ORACLE-INPUTS.json" => {
            "700284e276d7c82c6d12c7c0973fff78aabc7d3d5e916e06cacec3911643cbc5"
        }
        "extra-manual-numeric-01/ACTUAL-MANUAL-ORACLE.json" => {
            "a77b022c3cd58815940c82b0e6715a395ff75727e7e2666e9270d86fe695c482"
        }
        "ORIGINAL106-FROZEN-EXPECTATIONS.json" => {
            "fc5731fca4537595cbf3233c061c5861385e1c4e746e01a19c613cc230f13e24"
        }
        _ => panic!("unregistered original oracle input"),
    };
    let path = root.join(relative);
    let bytes = std::fs::read(&path).unwrap_or_else(|_| {
        panic!("missing original oracle {relative}; provide OE_TYPED_ORIGINAL_INPUT_ROOT")
    });
    assert_eq!(
        oe_core::sha256(&bytes),
        expected,
        "original oracle bytes changed: {relative}"
    );
    path
}

struct Fixture {
    outcomes: RefCell<VecDeque<Value>>,
    wires: RefCell<Vec<SemanticProviderWireRequest>>,
    deny_decoded: bool,
}
impl SemanticProviderTransportPort for Fixture {
    fn post(
        &self,
        wire: &SemanticProviderWireRequest,
    ) -> Result<SemanticProviderWireResponse, SemanticProviderFailure> {
        self.wires.borrow_mut().push(wire.clone());
        let next = self
            .outcomes
            .borrow_mut()
            .pop_front()
            .expect("unexpected actual dispatch");
        if next.get("error").is_some() {
            return Err(oe_semantic::providers::failure(
                "synthetic_original_transport_failure",
                "transport",
            ));
        }
        let body = json!({"choices":[{"message":{"content":next["raw"]}}]}).to_string();
        Ok(SemanticProviderWireResponse {
            http_status: 200,
            body_sha256: oe_core::sha256(body.as_bytes()),
            body,
            transport_kind: "injected_fixture".into(),
            transport_identity: "pinned-original-typed-manual17/1".into(),
        })
    }
}
impl SemanticProviderResultGuardPort for Fixture {
    fn check_result(
        &self,
        result: &SemanticProviderResult,
    ) -> Result<SemanticProviderResultGuard, SemanticProviderFailure> {
        if self.deny_decoded && result.output.is_object() {
            return Err(oe_semantic::providers::failure(
                "credential_material_in_response",
                "guard",
            ));
        }
        Ok(SemanticProviderResultGuard {
            schema: "semantica.provider-result-guard/1".into(),
            credential_material_absent: true,
        })
    }
}
fn request(case: &Value) -> Value {
    json!({"schema":"semantica.typed-generation-request/1", "model_id":format!("semantica.semantic_extract.schemas::{}",case["schema"].as_str().unwrap()),
    "profile":"semantica.pydantic-json/1","max_retries":case.get("retries").cloned().unwrap_or(json!(3)),
    "generation":{"schema":"semantica.provider-request/1","provider":"openai","interface_profile":"semantic_extract",
        "operation":"generate","model":null,"custom_endpoint":false,"prompt":"Extract records.","options":case.get("kwargs").cloned().unwrap_or(json!({}))}})
}
#[test]
fn all_original_manual17_keep_outputs_calls_feedback_and_delay() {
    let frozen: Value = serde_json::from_slice(
        &std::fs::read(original_input(
            "frozen-original-inputs/FROZEN-MANUAL-ORACLE-INPUTS.json",
        ))
        .unwrap(),
    )
    .unwrap();
    let original: Value = serde_json::from_slice(
        &std::fs::read(original_input(
            "frozen-original-inputs/ACTUAL-MANUAL-ORACLE.json",
        ))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(frozen["cases"].as_array().unwrap().len(), 17);
    let schema = RustSchemaRuntime::new().unwrap();
    for (case, observed) in frozen["cases"]
        .as_array()
        .unwrap()
        .iter()
        .zip(original["cases"].as_array().unwrap())
    {
        assert_eq!(case["case"], observed["case"]);
        let fixture = Fixture {
            outcomes: RefCell::new(case["outcomes"].as_array().unwrap().clone().into()),
            wires: RefCell::new(vec![]),
            deny_decoded: false,
        };
        let calls = SharedModelCallContext::new(SharedModelCallPolicy { max_calls: 20 }).unwrap();
        let dispatch = RustTypedDispatchHarness::new(&calls, &fixture, &fixture);
        let runtime = RustTypedRuntime::new(&schema, &dispatch);
        let parsed = serde_json::from_value::<SemanticTypedGenerationRequest>(request(case));
        // Original Python's range rejects a noninteger before dispatch. The
        // generated Rust i64 contract rejects the same logical retry value.
        if case["expected_exception"] == "TypeError" {
            assert!(parsed.is_err());
            assert_eq!(calls.snapshot().attempts, 0);
            continue;
        }
        let mut delays = vec![];
        let result = runtime
            .generate_with_delay(&parsed.unwrap(), |d| delays.push(d))
            .unwrap();
        assert_eq!(
            result.execution,
            if case["expected_status"] == "validated" {
                "completed"
            } else {
                "failed"
            },
            "{}",
            case["case"]
        );
        assert_eq!(
            fixture.wires.borrow().len() as u64,
            case["expected_calls"].as_u64().unwrap(),
            "{}",
            case["case"]
        );
        assert_eq!(
            calls.snapshot().attempts,
            case["expected_calls"].as_u64().unwrap(),
            "{}",
            case["case"]
        );
        assert_eq!(
            result.attempts.iter().filter(|a| a.reserved).count() as u64,
            calls.snapshot().attempts
        );
        assert_eq!(
            delays.iter().map(Duration::as_secs).sum::<u64>(),
            case.get("expected_actual_sleep_seconds")
                .and_then(Value::as_u64)
                .unwrap_or(0)
        );
        if let Some(expected) = case.get("expected_dump") {
            assert_eq!(&result.output, expected, "{}", case["case"]);
            assert_eq!(result.output, observed["dump"]);
        }
        for (wire, expected) in fixture
            .wires
            .borrow()
            .iter()
            .zip(observed["calls"].as_array().unwrap())
        {
            assert_eq!(
                wire.body["messages"][0]["content"], expected["prompt"],
                "{}",
                case["case"]
            );
        }
        if case["case"] == "none_and_zero_kwargs_preserved" {
            assert_eq!(result.attempts[0].request.options, case["kwargs"]);
            assert_eq!(fixture.wires.borrow()[0].body["max_tokens"], 0);
            assert_eq!(fixture.wires.borrow()[0].body["top_p"], 0);
        }
    }
}
#[test]
fn typed_repair_budget_and_second_decode_credential_refusals_stop_before_fallback() {
    let schema = RustSchemaRuntime::new().unwrap();
    for (max, deny) in [(0, false), (3, true)] {
        let fixture = Fixture {
            outcomes: RefCell::new(vec![json!({"raw":"{\"entities\":[]}"})].into()),
            wires: RefCell::new(vec![]),
            deny_decoded: deny,
        };
        let calls = SharedModelCallContext::new(SharedModelCallPolicy { max_calls: max }).unwrap();
        let dispatcher = RustTypedDispatchHarness::new(&calls, &fixture, &fixture);
        let runtime = RustTypedRuntime::new(&schema, &dispatcher);
        let input: SemanticTypedGenerationRequest =
            serde_json::from_value(request(&json!({"schema":"EntitiesResponse","retries":3})))
                .unwrap();
        let result = runtime
            .generate_with_delay(&input, |_| panic!("refusal must not retry/sleep"))
            .unwrap();
        assert_eq!(result.execution, "failed");
        assert_eq!(
            result.failure.unwrap().phase,
            if deny { "guard" } else { "budget" }
        );
        assert_eq!(calls.snapshot().attempts, if deny { 1 } else { 0 });
        assert_eq!(fixture.wires.borrow().len(), if deny { 1 } else { 0 });
        assert!(result.output.is_null());
        assert_eq!(result.attempts.len(), 1);
        assert!(result.attempts[0].result.is_none());
    }
}
#[test]
fn original_production_retry_delays_are_actual_sleep_not_just_receipt_metadata() {
    let evidence = original_input("frozen-original-inputs/FROZEN-MANUAL-ORACLE-INPUTS.json");
    let frozen: Value = serde_json::from_slice(&std::fs::read(evidence).unwrap()).unwrap();
    let schema = RustSchemaRuntime::new().unwrap();
    for name in [
        "default_three_retries_validation_errors_actual_backoff",
        "validation_feedback_then_success_actual_backoff",
    ] {
        let case = frozen["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["case"] == name)
            .unwrap();
        let fixture = Fixture {
            outcomes: RefCell::new(case["outcomes"].as_array().unwrap().clone().into()),
            wires: RefCell::new(vec![]),
            deny_decoded: false,
        };
        let calls = SharedModelCallContext::new(SharedModelCallPolicy { max_calls: 20 }).unwrap();
        let dispatcher = RustTypedDispatchHarness::new(&calls, &fixture, &fixture);
        let runtime = RustTypedRuntime::new(&schema, &dispatcher);
        let start = std::time::Instant::now();
        let result = runtime
            .generate_typed(&serde_json::from_value(request(case)).unwrap())
            .unwrap();
        assert!(
            start.elapsed()
                >= Duration::from_secs(case["expected_actual_sleep_seconds"].as_u64().unwrap())
        );
        assert_eq!(
            calls.snapshot().attempts,
            case["expected_calls"].as_u64().unwrap()
        );
        assert_eq!(
            result.execution,
            if case["expected_status"] == "validated" {
                "completed"
            } else {
                "failed"
            }
        );
    }
}

#[test]
fn real_shared_harness_returns_every_generated_strong_model_without_extra_calls() {
    let path = original_input("ORIGINAL106-FROZEN-EXPECTATIONS.json");
    let frozen: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let cases = frozen["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["case"] == "minimal")
        .collect::<Vec<_>>();
    assert_eq!(cases.len(), 106);
    let fixture = Fixture {
        outcomes: RefCell::new(
            cases
                .iter()
                .map(|c| json!({"raw":serde_json::to_string(&c["input"]).unwrap()}))
                .collect(),
        ),
        wires: RefCell::new(vec![]),
        deny_decoded: false,
    };
    let calls = SharedModelCallContext::new(SharedModelCallPolicy { max_calls: 107 }).unwrap();
    let dispatch = RustTypedDispatchHarness::new(&calls, &fixture, &fixture);
    let schema = RustSchemaRuntime::new().unwrap();
    let runtime = RustTypedRuntime::new(&schema, &dispatch);
    let template: SemanticTypedGenerationRequest =
        serde_json::from_value(request(&json!({"schema":"EntitiesResponse","retries":1}))).unwrap();
    for (index, case) in cases.iter().enumerate() {
        let (snapshot, receipt) = runtime
            .generate_registered_snapshot(
                template.generation.clone(),
                case["model"].as_str().unwrap(),
                1,
            )
            .unwrap();
        assert_eq!(snapshot.model_id(), case["model"]);
        assert_eq!(
            serde_json::to_value(snapshot).unwrap(),
            case["original"]["output"]
        );
        assert_eq!(receipt.attempts.len(), 1);
        assert!(receipt.attempts[0].reserved);
        assert_eq!(calls.snapshot().attempts, index as u64 + 1);
    }
    fixture
        .outcomes
        .borrow_mut()
        .push_back(json!({"raw":"{\"text\":\" A \",\"label\":\"part\"}"}));
    let (entity, receipt) = runtime
        .generate_snapshot::<SemanticModelSemanticaSemanticExtractSchemasEntityOut>(
            template.generation,
            1,
        )
        .unwrap();
    assert_eq!(entity.text, "A");
    assert_eq!(entity.label, "part");
    assert_eq!(receipt.attempts.len(), 1);
    assert_eq!(calls.snapshot().attempts, 107);
    assert_eq!(fixture.wires.borrow().len(), 107);
}

#[test]
fn self_reported_completed_dispatch_with_changed_source_never_returns_a_model() {
    struct Changed;
    impl SemanticTypedDispatchPort for Changed {
        fn dispatch_typed(
            &self,
            input: &SemanticTypedDispatchRequest,
        ) -> Result<SemanticTypedDispatchResult, SemanticProviderFailure> {
            Ok(SemanticTypedDispatchResult{schema:"semantica.typed-dispatch-result/1".into(),execution:"completed".into(),
                output:json!({"entities":[]}),reserved:true,failure:None,response:Some(serde_json::from_value(json!({
                    "schema":"semantica.provider-result/1","execution":"completed","provider":input.generation.provider,"operation":"generate",
                    "output":"{\"entities\":[{\"text\":\"different\",\"label\":\"part\"}]}","request_sha256":"0".repeat(64),
                    "wire_request_sha256":"0".repeat(64),"response_sha256":"0".repeat(64),"requested_model":null,"resolved_model":null,
                    "provider_response_id":null,"transport_identity":"synthetic-mismatch","transport_kind":"injected_fixture",
                    "usage":{"input_tokens":null,"output_tokens":null,"total_tokens":null},"usage_status":"unknown"
                })).unwrap())})
        }
    }
    let schema = RustSchemaRuntime::new().unwrap();
    let dispatcher = Changed;
    let runtime = RustTypedRuntime::new(&schema, &dispatcher);
    let input: SemanticTypedGenerationRequest =
        serde_json::from_value(request(&json!({"schema":"EntitiesResponse","retries":1}))).unwrap();
    assert_eq!(
        runtime
            .generate_with_delay(&input, |_| panic!("invalid source cannot retry"))
            .unwrap_err()
            .code,
        "typed_dispatch_source_mismatch"
    );
}

#[test]
fn original_schema_exceptions_retry_on_the_same_budget_without_validation_feedback() {
    let frozen: Value = serde_json::from_slice(
        &std::fs::read(original_input(
            "extra-manual-numeric-01/FROZEN-MANUAL-ORACLE-INPUTS.json",
        ))
        .unwrap(),
    )
    .unwrap();
    let original: Value = serde_json::from_slice(
        &std::fs::read(original_input(
            "extra-manual-numeric-01/ACTUAL-MANUAL-ORACLE.json",
        ))
        .unwrap(),
    )
    .unwrap();
    let schema = RustSchemaRuntime::new().unwrap();
    for (case, observed) in frozen["cases"]
        .as_array()
        .unwrap()
        .iter()
        .zip(original["cases"].as_array().unwrap())
    {
        let fixture = Fixture {
            outcomes: RefCell::new(case["outcomes"].as_array().unwrap().clone().into()),
            wires: RefCell::new(vec![]),
            deny_decoded: false,
        };
        let calls = SharedModelCallContext::new(SharedModelCallPolicy { max_calls: 5 }).unwrap();
        let dispatch = RustTypedDispatchHarness::new(&calls, &fixture, &fixture);
        let runtime = RustTypedRuntime::new(&schema, &dispatch);
        let input: SemanticTypedGenerationRequest = serde_json::from_value(request(case)).unwrap();
        let mut delays = vec![];
        let output = runtime
            .generate_with_delay(&input, |delay| delays.push(delay))
            .unwrap();
        assert_eq!(
            output.execution,
            if observed["status"] == "validated" {
                "completed"
            } else {
                "failed"
            }
        );
        assert_eq!(
            calls.snapshot().attempts,
            case["expected_calls"].as_u64().unwrap()
        );
        assert_eq!(
            fixture.wires.borrow().len() as u64,
            calls.snapshot().attempts
        );
        assert_eq!(
            delays.iter().map(Duration::as_secs).sum::<u64>(),
            case.get("expected_actual_sleep_seconds")
                .and_then(Value::as_u64)
                .unwrap_or(0)
        );
        if let Some(dump) = observed.get("dump") {
            assert_eq!(output.output, *dump);
        }
        for (wire, old) in fixture
            .wires
            .borrow()
            .iter()
            .zip(observed["calls"].as_array().unwrap())
        {
            assert_eq!(wire.body["messages"][0]["content"], old["prompt"]);
            assert!(
                !wire.body["messages"][0]["content"]
                    .as_str()
                    .unwrap()
                    .contains("Previous response was invalid JSON")
            );
        }
    }
}

#[test]
fn typed_and_plain_chat_share_one_existing_attempt_authority() {
    let calls = SharedModelCallContext::new(SharedModelCallPolicy { max_calls: 2 }).unwrap();
    let initial = Fixture {
        outcomes: RefCell::new(vec![json!({"raw":"seed"})].into()),
        wires: RefCell::new(vec![]),
        deny_decoded: false,
    };
    let plain: SemanticProviderRequest = serde_json::from_value(json!({"schema":"semantica.provider-request/1","provider":"openai","interface_profile":"semantic_extract","operation":"generate","model":null,"custom_endpoint":false,"prompt":"Synthetic seed","options":{}})).unwrap();
    oe_engineering::harness::generate_provider(&calls, &plain, &initial, &initial, 1).unwrap();
    let fixture = Fixture {
        outcomes: RefCell::new(
            vec![json!({"raw":"{\"entities\":[{\"text\":\"A\",\"label\":42}]}"})].into(),
        ),
        wires: RefCell::new(vec![]),
        deny_decoded: false,
    };
    let schema = RustSchemaRuntime::new().unwrap();
    let dispatch = RustTypedDispatchHarness::new(&calls, &fixture, &fixture);
    let typed = RustTypedRuntime::new(&schema, &dispatch);
    let mut delays = vec![];
    let output = typed
        .generate_with_delay(
            &serde_json::from_value(request(&json!({"schema":"EntitiesResponse","retries":3})))
                .unwrap(),
            |x| delays.push(x),
        )
        .unwrap();
    assert_eq!(
        output.failure.unwrap().code,
        "shared_model_call_budget_exhausted"
    );
    assert_eq!(calls.snapshot().attempts, 2);
    assert_eq!(fixture.wires.borrow().len(), 1);
    assert_eq!(output.attempts.len(), 2);
    assert!(!output.attempts[1].reserved);
    assert_eq!(delays, vec![Duration::from_secs(1)]);
}
