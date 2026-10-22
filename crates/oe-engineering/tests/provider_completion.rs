use oe_contracts::{BaseStructuredRequest, ProviderTraceContext, SharedModelCallPolicy};
use oe_engineering::harness::*;
use semantica_contracts::*;
use serde_json::{Value, json};
use std::{cell::RefCell, collections::VecDeque, time::Duration};
fn chat() -> SemanticProviderRequest {
    serde_json::from_value(json!({"schema":"semantica.provider-request/1",
    "provider":"openai","interface_profile":"semantic_extract","operation":"generate","model":null,
    "custom_endpoint":false,"prompt":"please json","options":{}}))
    .unwrap()
}
fn embedding(text: &str) -> SemanticEmbeddingRequest {
    serde_json::from_value(json!({"schema":"semantica.embedding-request/1",
    "provider":"openai","interface_profile":"provider_store","model":null,"custom_endpoint":false,
    "input":text,"options":{}}))
    .unwrap()
}
fn base(max: i64) -> BaseStructuredRequest {
    BaseStructuredRequest {
        schema: "oev2.base-structured-request/1".into(),
        generation: chat(),
        max_retries: max,
    }
}
fn context(max: u64) -> SharedModelCallContext {
    SharedModelCallContext::new(SharedModelCallPolicy { max_calls: max }).unwrap()
}
enum Reply {
    Chat(&'static str),
    Embedding,
    Fail,
}
struct Fixture {
    replies: RefCell<VecDeque<Reply>>,
    requests: RefCell<Vec<SemanticProviderWireRequest>>,
}
impl Fixture {
    fn new(replies: Vec<Reply>) -> Self {
        Self {
            replies: RefCell::new(replies.into()),
            requests: RefCell::new(vec![]),
        }
    }
}
impl SemanticProviderTransportPort for Fixture {
    fn post(
        &self,
        input: &SemanticProviderWireRequest,
    ) -> Result<SemanticProviderWireResponse, SemanticProviderFailure> {
        self.requests.borrow_mut().push(input.clone());
        let data = match self
            .replies
            .borrow_mut()
            .pop_front()
            .expect("fixture unexpected dispatch")
        {
            Reply::Chat(content) => json!({"choices":[{"message":{"content":content}}]}),
            Reply::Embedding => {
                json!({"data":[{"embedding":[0.1,-0.2]}],"usage":{"prompt_tokens":2,"total_tokens":2}})
            }
            Reply::Fail => {
                return Err(oe_semantic::providers::failure(
                    "synthetic_http_failure",
                    "transport",
                ));
            }
        };
        let body = data.to_string();
        Ok(SemanticProviderWireResponse {
            http_status: 200,
            body_sha256: oe_core::sha256(body.as_bytes()),
            body,
            transport_kind: "injected_fixture".into(),
            transport_identity: "provider-completion-fixture/1".into(),
        })
    }
}
impl SemanticProviderResultGuardPort for Fixture {
    fn check_result(
        &self,
        result: &SemanticProviderResult,
    ) -> Result<SemanticProviderResultGuard, SemanticProviderFailure> {
        if result.output.get("secret").is_some() {
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
#[test]
fn original_base_zero_false_retry_and_final_false_do_not_sleep() {
    let calls = context(3);
    let fixture = Fixture::new(vec![
        Reply::Chat("false"),
        Reply::Chat("{}"),
        Reply::Chat("{\"x\":1}"),
    ]);
    let mut sleeps = vec![];
    let result = generate_base_structured_with_delay(&calls, &base(3), &fixture, &fixture, |d| {
        sleeps.push(d)
    })
    .unwrap();
    assert_eq!(result.output, json!({"x":1}));
    assert_eq!(result.execution, "completed");
    assert!(result.failure.is_none());
    assert_eq!(result.attempts.len(), 3);
    assert!(result.attempts.iter().all(|a| a.reserved));
    assert!(sleeps.is_empty());
    assert_eq!(calls.snapshot().attempts, 3);
    assert_eq!(calls.snapshot().unreported_attempts, 3);
    for wire in fixture.requests.borrow().iter() {
        assert_eq!(
            wire.body["messages"][0]["content"],
            "please json\n\nReturn the response as valid JSON only."
        );
        assert!(wire.body.get("response_format").is_none());
    }
    let no = Fixture::new(vec![]);
    let calls = context(0);
    let result =
        generate_base_structured_with_delay(&calls, &base(0), &no, &no, |_| panic!("zero sleeps"))
            .unwrap();
    assert_eq!(result.output, json!([]));
    assert!(result.attempts.is_empty());
    assert_eq!(calls.snapshot().attempts, 0);
    for max in [-1, i64::MIN] {
        let result = generate_base_structured_with_delay(&calls, &base(max), &no, &no, |_| {
            panic!("negative sleeps")
        })
        .unwrap();
        assert_eq!(result.output, json!([]));
        assert!(result.attempts.is_empty());
        assert_eq!(calls.snapshot().attempts, 0);
    }
    let fixture = Fixture::new(vec![Reply::Chat("false"), Reply::Chat("false")]);
    let mut input = base(2);
    input.generation.prompt = "JSON requested".into();
    let result =
        generate_base_structured_with_delay(&context(2), &input, &fixture, &fixture, |_| {
            panic!("falsy never sleeps")
        })
        .unwrap();
    assert_eq!(result.output, json!(false));
    assert_eq!(
        fixture.requests.borrow()[0].body["messages"][0]["content"],
        "JSON requested"
    );
}
#[test]
fn original_base_error_backoff_keeps_logical_request_and_all_attempts_distinct() {
    let fixture = Fixture::new(vec![
        Reply::Chat("garbage"),
        Reply::Chat("bad"),
        Reply::Chat("{\"x\":1}"),
    ]);
    let calls = context(3);
    let input = base(3);
    let mut sleeps = vec![];
    let result =
        generate_base_structured_with_delay(&calls, &input, &fixture, &fixture, |d| sleeps.push(d))
            .unwrap();
    assert_eq!(sleeps, vec![Duration::from_secs(2), Duration::from_secs(4)]);
    assert_eq!(result.output, json!({"x":1}));
    assert!(result.failure.is_none());
    assert_eq!(
        result.logical_request_sha256,
        oe_core::sha256(serde_json::to_vec(&input).unwrap())
    );
    for attempt in &result.attempts {
        let generation = attempt.result.as_ref().unwrap();
        assert_eq!(generation.operation, "generate");
        assert_eq!(
            generation.request_sha256,
            oe_core::sha256(serde_json::to_vec(&attempt.request).unwrap())
        );
        assert_ne!(generation.request_sha256, result.logical_request_sha256);
    }
    assert!(result.attempts[0].failure.is_some());
    assert!(result.attempts[2].failure.is_none());
}
#[test]
fn base_budget_and_second_decode_refusals_halt_without_more_dispatch_or_unsafe_receipt() {
    let fixture = Fixture::new(vec![]);
    let calls = context(0);
    let result = generate_base_structured_with_delay(&calls, &base(3), &fixture, &fixture, |_| {
        panic!("refusal never sleeps")
    })
    .unwrap();
    assert_eq!(result.execution, "failed");
    assert_eq!(
        result.failure.unwrap().code,
        "shared_model_call_budget_exhausted"
    );
    assert_eq!(result.attempts.len(), 1);
    assert!(!result.attempts[0].reserved);
    assert!(fixture.requests.borrow().is_empty());
    let fixture = Fixture::new(vec![Reply::Chat(r#"{"secret":"\u0073k-SYNTHETIC_ONLY"}"#)]);
    let result =
        generate_base_structured_with_delay(&context(3), &base(3), &fixture, &fixture, |_| {
            panic!("guard never sleeps")
        })
        .unwrap();
    assert_eq!(result.execution, "failed");
    assert!(result.output.is_null());
    assert!(result.attempts[0].result.is_none());
    let serialized = serde_json::to_string(&result).unwrap();
    assert!(!serialized.contains("SYNTHETIC_ONLY"));
    assert_eq!(fixture.requests.borrow().len(), 1);
}

#[test]
fn original_embedding_batch_failure_stops_following_items_and_empty_batch_is_float64() {
    let fixture = Fixture::new(vec![Reply::Embedding, Reply::Fail]);
    let calls = context(3);
    let input = SemanticEmbeddingBatchRequest {
        schema: "semantica.embedding-batch-request/1".into(),
        requests: vec![embedding("first"), embedding("second"), embedding("third")],
    };
    let batch = embed_provider_batch(&calls, &input, &fixture, &fixture).unwrap();
    assert_eq!(batch.execution, "failed");
    assert!(batch.output.is_none());
    assert_eq!(batch.failed_index, Some(1));
    assert_eq!(batch.results.len(), 1);
    assert_eq!(fixture.requests.borrow().len(), 2);
    assert!(batch.attempts[1].reserved);
    assert_eq!(calls.snapshot().attempts, 2);
    assert_eq!(calls.snapshot().unreported_attempts, 2);
    let empty = SemanticEmbeddingBatchRequest {
        schema: "semantica.embedding-batch-request/1".into(),
        requests: vec![],
    };
    let result = embed_provider_batch(&context(0), &empty, &fixture, &fixture).unwrap();
    assert_eq!(result.execution, "completed");
    let tensor = result.output.unwrap();
    assert_eq!(tensor.dtype, "float64");
    assert_eq!(tensor.shape, vec![0]);
    assert_eq!(fixture.requests.borrow().len(), 2);
}
#[test]
fn embedding_provenance_uses_same_dataset_and_rejects_changed_input_bytes() {
    let calls = context(2);
    let fixture = Fixture::new(vec![Reply::Embedding, Reply::Chat("answer")]);
    let request = embedding("source text");
    let result = embed_provider(&calls, &request, &fixture, &fixture).unwrap();
    let mut session = oe_semantic::providers::DatasetSession::new();
    let mut trace = ProviderTraceContext {
        graph_id: "urn:synthetic:combined:graph".into(),
        activity_id: "urn:synthetic:embed:activity".into(),
        agent_id: "urn:synthetic:model:agent".into(),
        record_id: "urn:synthetic:embed:record".into(),
        output_id: "urn:synthetic:embed:output".into(),
        started_at: None,
        ended_at: None,
    };
    embedding_trace(&mut session, &request, &result, &trace, &fixture).unwrap();
    let before = session.snapshot().unwrap();
    assert!(before.canonical_nquads.contains("embedding_generation"));
    let mut changed = request.clone();
    changed.input = "other text".into();
    assert!(embedding_trace(&mut session, &changed, &result, &trace, &fixture).is_err());
    assert_eq!(session.snapshot().unwrap(), before);
    let chat = chat();
    let response = generate_provider(&calls, &chat, &fixture, &fixture, 1).unwrap();
    trace.activity_id = "urn:synthetic:chat:activity".into();
    trace.record_id = "urn:synthetic:chat:record".into();
    trace.output_id = "urn:synthetic:chat:output".into();
    provider_trace(&mut session, &chat, &response, &trace, &fixture).unwrap();
    let snapshot = session.snapshot().unwrap();
    assert_eq!(snapshot.revision, 2);
    assert!(snapshot.canonical_nquads.contains("embedding_generation"));
    assert!(snapshot.canonical_nquads.contains("llm_generation"));
    assert!(snapshot.canonical_nquads.contains("not_determined"));
    assert_eq!(calls.snapshot().attempts, 2);
}

#[test]
fn same_context_counts_chat_and_each_embedding_item_before_budget_refusal() {
    let calls = context(3);
    let initial = Fixture::new(vec![Reply::Chat("initial")]);
    generate_provider(&calls, &chat(), &initial, &initial, 1).unwrap();
    let chat_fixture = Fixture::new(vec![Reply::Chat("answer")]);
    generate_provider(&calls, &chat(), &chat_fixture, &chat_fixture, 1).unwrap();
    let embed_fixture = Fixture::new(vec![Reply::Embedding]);
    let input = SemanticEmbeddingBatchRequest {
        schema: "semantica.embedding-batch-request/1".into(),
        requests: vec![embedding("first"), embedding("second"), embedding("third")],
    };
    let batch = embed_provider_batch(&calls, &input, &embed_fixture, &embed_fixture).unwrap();
    assert_eq!(batch.execution, "failed");
    assert_eq!(batch.failed_index, Some(1));
    assert!(batch.output.is_none());
    assert_eq!(batch.results.len(), 1);
    assert_eq!(batch.attempts.len(), 2);
    assert!(!batch.attempts[1].reserved);
    assert_eq!(batch.attempts[1].request.input, "second");
    assert_eq!(embed_fixture.requests.borrow().len(), 1);
    let state = calls.snapshot();
    assert_eq!(state.attempts, 3);
    assert_eq!(state.known_input_tokens, "2");
    assert_eq!(state.known_output_tokens, "0");
    assert_eq!(state.unreported_attempts, 3);
    assert_eq!(
        embed_provider(&calls, &embedding("fourth"), &embed_fixture, &embed_fixture)
            .unwrap_err()
            .code,
        "shared_model_call_budget_exhausted"
    );
    assert_eq!(embed_fixture.requests.borrow().len(), 1);
}
