//! Source checking around the existing typed transport and shared-call owner.
//! This is a borrowed guard/receipt adapter, never a second dispatcher or budget.
use crate::harness::{RustTypedDispatchHarness, SharedModelCallContext};
use anyhow::{Context, Result, ensure};
use oe_contracts::{KnowledgeTypedExtractionInput8 as Input, KnowledgeTypedExtractionReceipt8};
use oe_knowledge::typed_source8::PreparedExtractionSource8;
use oe_semantic::{
    schema::RustSchemaRuntime,
    typed_relations8::{PROFILE8, RustTypedRelationExtraction8},
};
use semantica_contracts::*;
use serde_json::{Value, json};
use std::{cell::RefCell, time::Instant};
fn fail(code: &str) -> SemanticProviderFailure {
    oe_semantic::providers::failure(code, "guard")
}
/// Check actual bytes at the final existing HTTP post boundary, including
/// repair/empty-result fallback requests. Both caps are caller-selected policy.
pub fn check_post8(
    source: &PreparedExtractionSource8<'_>,
    wire: &SemanticProviderWireRequest,
) -> Result<Value> {
    source.verify_current8()?;
    let input = source.input();
    let bytes = serde_json::to_vec(&wire.body)?;
    ensure!(
        wire.validate().is_ok() && oe_core::sha256(&bytes) == wire.body_sha256,
        "typed_extraction8_wire_identity"
    );
    ensure!(
        input.maximum_request_bytes > 0
            && input.maximum_request_bytes <= 2 * 1024 * 1024
            && bytes.len() as u64 <= input.maximum_request_bytes,
        "typed_extraction8_request_byte_budget"
    );
    let output = wire.body["max_tokens"]
        .as_u64()
        .context("typed_extraction8_explicit_output_budget")?;
    ensure!(
        output > 0 && output <= input.reserved_output_tokens,
        "typed_extraction8_output_reserve"
    );
    let total = (bytes.len() as u64)
        .checked_add(input.reserved_output_tokens)
        .context("typed_extraction8_token_overflow")?;
    ensure!(
        total <= input.context_window_tokens,
        "typed_extraction8_context_token_budget"
    );
    let content = wire.body["messages"][0]["content"]
        .as_str()
        .context("typed_extraction8_complete_text")?;
    ensure!(
        content.contains(source.text()),
        "typed_extraction8_necessary_text_removed"
    );
    ensure!(
        Instant::now() < source.deadline(),
        "typed_extraction8_source_deadline"
    );
    Ok(
        json!({"request_bytes":bytes.len(),"maximum_request_bytes":input.maximum_request_bytes,
        "conservative_input_tokens":bytes.len(),"reserved_output_tokens":input.reserved_output_tokens,
        "conservative_total_tokens":total,"context_window_tokens":input.context_window_tokens,
        "estimator":"full-UTF8-wire-bytes-conservative/8","exact_tokenizer":"unknown","complete_selected_window":true}),
    )
}
pub struct CheckedExtractionTransport8<'a> {
    pub source: &'a PreparedExtractionSource8<'a>,
    pub inner: &'a dyn SemanticProviderTransportPort,
    observations: RefCell<Vec<Value>>,
}
impl<'a> CheckedExtractionTransport8<'a> {
    pub fn new(
        source: &'a PreparedExtractionSource8<'a>,
        inner: &'a dyn SemanticProviderTransportPort,
    ) -> Self {
        Self {
            source,
            inner,
            observations: RefCell::new(vec![]),
        }
    }
    pub fn observations(&self) -> Value {
        json!(*self.observations.borrow())
    }
}
impl SemanticProviderTransportPort for CheckedExtractionTransport8<'_> {
    fn post(
        &self,
        wire: &SemanticProviderWireRequest,
    ) -> std::result::Result<SemanticProviderWireResponse, SemanticProviderFailure> {
        let budget = match check_post8(self.source, wire) {
            Ok(x) => x,
            Err(e) => {
                self.observations.borrow_mut().push(
                    json!({"wire":wire,"http_started":false,"response":null,"error":e.to_string()}),
                );
                return Err(fail("typed_extraction8_pre_send_source_or_budget"));
            }
        };
        let result = self.inner.post(wire);
        let current = self.source.verify_current8();
        // The existing concrete HTTP transport already guards raw/decoded
        // secret material. Retain safe raw response even when sources drift.
        self.observations.borrow_mut().push(json!({"wire":wire,"budget":budget,"http_started":true,
            "response":result.as_ref().ok(),"transport_failure":result.as_ref().err(),
            "source_current_after":current.is_ok(),"source_failure_after":current.as_ref().err().map(ToString::to_string)}));
        if current.is_err() {
            return Err(fail("typed_extraction8_post_send_source_changed"));
        }
        result
    }
}
pub fn run_source8<'a>(
    source: &'a PreparedExtractionSource8<'a>,
    calls: &SharedModelCallContext,
    transport: &dyn SemanticProviderTransportPort,
    guard: &dyn SemanticProviderResultGuardPort,
) -> Result<KnowledgeTypedExtractionReceipt8> {
    source.verify_current8()?;
    let input: &Input = source.input();
    let checked = CheckedExtractionTransport8::new(source, transport);
    let dispatch = RustTypedDispatchHarness::new(calls, &checked, guard);
    let schema = RustSchemaRuntime::new().map_err(|e| anyhow::anyhow!(e.code))?;
    let consumer = RustTypedRelationExtraction8 {
        schema: &schema,
        dispatch: &dispatch,
    };
    let request = SemanticTypedRelationRequest8 {
        schema: "semantica.typed-relation-request/8".into(),
        text: source.text().into(),
        entities: input.entities.clone(),
        relation_types: input.relation_types.clone(),
        temporal: input.temporal,
        generation: input.generation.clone(),
        max_retries: input.max_retries,
    };
    let mut outcome = match consumer.extract_typed_relations8(&request) {
        Ok(x) => x,
        Err(e) => SemanticTypedRelationResult8 {
            schema: "semantica.typed-relation-result/8".into(),
            execution: "failed".into(),
            profile: PROFILE8.into(),
            source_utf8_sha256: oe_core::sha256(source.text()),
            typed: None,
            fallback: None,
            relations: json!([]),
            failure: Some(e),
            unknown: vec![],
        },
    };
    let current = source.verify_current8().is_ok();
    if !current {
        outcome.execution = "failed".into();
        outcome.failure = Some(fail("typed_extraction8_final_source_changed"));
    }
    let execution = if current && outcome.execution == "completed" {
        "completed"
    } else {
        "failed"
    };
    Ok(KnowledgeTypedExtractionReceipt8{schema:"oev2.source-bound-typed-extraction-receipt/8".into(),execution:execution.into(),
        source_current:current,source:source.report().clone(),outcome,wire_observations:checked.observations(),
        calls:serde_json::to_value(calls.snapshot())?,unknown:vec!["actual model quality/calibration/causal/Chinese/engineering adoption and total provider cost unknown; synthetic loopback is not external model inference".into(),
            "same absolute caller deadline checked at stages; synchronous source reads and blocking HTTP hard preemption remain unverified".into(),
            "source metadata is local provenance; original prompt contains selected text/entities and is not a full Project engineering advisory".into(),
            "originalPattern1/full13494/19modules/19protocols/22owners and RRSI full eligibility/use/feedback/recovery remain unchanged".into()]})
}
