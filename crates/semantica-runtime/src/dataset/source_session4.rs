//! Explicit version4 composes the existing action, observation and writer owners.
use super::super::{
    source_namespace_witness, source_observation, source_storage_witness, source_turtle_transcript,
};
use super::*;
use anyhow::bail;
use semantica_contracts::{
    SemanticSourceNamespaceCheckpoint4 as Checkpoint, SemanticSourceSessionPortV4,
    SemanticSourceSessionRequest4 as Request4, SemanticSourceSessionResult4 as Result4,
    SemanticSourceSessionStepWitness4 as StepWitness, SemanticSourceSessionTrace4 as Trace,
    SemanticSourceStateWitness3 as SourceState,
};
use std::time::{Duration, Instant};

fn view(r: &Request4) -> RequestView<'_> {
    RequestView {
        schema: &r.schema,
        profile: &r.profile,
        created_at: &r.created_at,
        cache_queries: r.cache_queries,
        resources: &r.resources,
        actions: r.actions.iter().map(ActionView::v2).collect(),
    }
}
pub fn validate_request4(r: &Request4) -> Result<()> {
    ensure!(
        r.validate().is_ok()
            && r.schema == "semantica.source-session-request/4"
            && r.profile == "semantica.rdflib760-source-session/4",
        "source_session_request_profile"
    );
    validate_common(&view(r))?;
    size(r, MAX_WIRE_BYTES)?;
    Ok(())
}
pub fn request_sha256_4(r: &Request4) -> Result<String> {
    Ok(crate::sha256(&semantica_json::ordered_json(
        &serde_json::to_value(r)?,
    )?))
}
pub fn source_bindings_sha256_4(r: &Request4) -> Result<String> {
    let bindings: Vec<_> = r.resources.iter().map(|r| &r.binding).collect();
    Ok(crate::sha256(&semantica_json::ordered_json(
        &serde_json::to_value(bindings)?,
    )?))
}
fn deadline(at: Instant) -> Result<()> {
    ensure!(Instant::now() < at, "source_session_receiving_deadline");
    Ok(())
}
fn capture_failure(error: &anyhow::Error) -> SemanticSourceSessionFailure {
    failure("source_session_result_byte_budget", "capture", error)
}
fn unattempted() -> Checkpoint {
    Checkpoint {
        status: "not_attempted".into(),
        state: None,
        failure: None,
        retained_bytes: None,
        retained_entries: None,
    }
}
fn checkpoint(s: &DatasetSession, limit: usize, at: Instant) -> Checkpoint {
    let (bytes, entries) = s.source_namespaces.retained_charge();
    let mut c = Checkpoint {
        retained_bytes: Some(bytes as u64),
        retained_entries: Some(entries as u64),
        ..unattempted()
    };
    match s.source_namespaces.capture_until(limit, at) {
        Ok(state) => {
            c.status = "captured".into();
            c.state = Some(state);
        }
        Err(e) => {
            c.status = "failed".into();
            c.failure = Some(capture_failure(&e));
        }
    }
    c
}
fn checked_checkpoint(s: &DatasetSession, limit: usize, at: Instant) -> Result<Checkpoint> {
    let c = checkpoint(s, limit, at);
    ensure!(
        c.status == "captured",
        "source_session4_namespace_capture_failed: {:?}",
        c.failure
    );
    Ok(c)
}
fn empty_witness(id: &str) -> StepWitness {
    StepWitness {
        action_id: id.into(),
        action_started: false,
        complete: false,
        before: None,
        after_action: None,
        after_observation: None,
        source: None,
        turtle: None,
    }
}
fn empty_step(a: &ActionView<'_>, revision: u64) -> SemanticSourceSessionStep2 {
    SemanticSourceSessionStep2 {
        id: a.id.into(),
        operation: a.operation.into(),
        status: "skipped".into(),
        backend_phase: "not_applicable".into(),
        revision_before: revision,
        revision_after: revision,
        result: Value::Null,
        failure: None,
        after_state: None,
        after_state_failure: None,
        source_state: Value::Null,
        source_state_failure: None,
        resource_requests: Vec::new(),
        resource_requests_complete: true,
    }
}
fn observed(s: &mut DatasetSession, limit: usize, at: Instant) -> Result<(Value, SourceState)> {
    deadline(at)?;
    let (value, typed) = source_observation::state_captured_until(s, Some(limit), Some(at))?;
    source_storage_witness::size_until(&value, crate::MAX_BYTES.min(limit), at)?;
    Ok((value, typed.context("source_session4_typed_state_missing")?))
}

fn execute4(r: &Request4) -> Result<Result4, SemanticRuntimeFailure> {
    execute4_bounded(
        r,
        Instant::now() + Duration::from_millis(DEADLINE_MS),
        MAX_RESULT_BYTES,
    )
}
fn execute4_bounded(
    r: &Request4,
    at: Instant,
    limit: usize,
) -> Result<Result4, SemanticRuntimeFailure> {
    validate_request4(r)
        .and_then(|_| deadline(at))
        .map_err(|e| contract_failure("source_session_admission_refused", e))?;
    let request_sha256 = request_sha256_4(r)
        .map_err(|e| contract_failure("source_session_request_identity_failed", e))?;
    let source_bindings_sha256 = source_bindings_sha256_4(r)
        .map_err(|e| contract_failure("source_session_source_identity_failed", e))?;
    let input = view(r);
    let limit = limit.min(MAX_RESULT_BYTES);
    let mut s = DatasetSession::new_source_update_internal(input.cache_queries);
    let mut trace = Trace {
        initial_before: checkpoint(&s, limit, at),
        initial_after: unattempted(),
        initial_source: None,
        steps: Vec::new(),
        final_namespace: unattempted(),
    };
    let mut initial_state = Value::Null;
    let mut initial_state_failure = None;
    let mut retained = 0usize;
    let mut budget_failure = None;
    let initial = (|| -> Result<()> {
        ensure!(
            trace.initial_before.status == "captured",
            "source_session4_initial_capture_failed"
        );
        let (value, typed) = observed(&mut s, limit, at)?;
        initial_state = value;
        trace.initial_source = Some(typed);
        trace.initial_after = checked_checkpoint(&s, limit, at)?;
        retained = source_storage_witness::size_until(
            &(
                &initial_state,
                &trace.initial_before,
                &trace.initial_after,
                &trace.initial_source,
            ),
            limit,
            at,
        )?;
        Ok(())
    })();
    if let Err(e) = initial {
        let f = capture_failure(&e);
        initial_state = Value::Null;
        initial_state_failure = Some(f.clone());
        trace.initial_source = None;
        trace.initial_before = unattempted();
        trace.initial_after = unattempted();
        budget_failure = Some(f);
    }
    let mut all_stopped = budget_failure.is_some();
    let mut failed = false;
    let mut observation_failed = initial_state_failure.is_some();
    let mut steps = Vec::new();
    let mut storage_witnesses = Vec::new();
    for action in &input.actions {
        let mut step = empty_step(action, s.revision());
        let mut w = empty_witness(action.id);
        if all_stopped || (failed && matches!(action.operation, "load" | "update" | "clear")) {
            steps.push(step);
            trace.steps.push(w);
            continue;
        }
        let mut storage = SemanticSourceSessionStorageWitness3 {
            action_id: action.id.into(),
            serialization: None,
            current_snapshot: None,
            current_source: None,
        };
        let left = limit.saturating_sub(retained);
        let capture = (|| -> Result<()> {
            w.before = Some(checked_checkpoint(&s, left, at)?);
            deadline(at)?;
            w.action_started = true;
            let mut reached = false;
            let outcome = execute_action(
                &mut s,
                &input,
                action,
                &steps,
                &mut step,
                &mut storage,
                true,
                MAX_RESULT_BYTES.saturating_sub(left),
                &mut reached,
                Some(&mut w.turtle),
                Some(at),
            );
            step.revision_after = s.revision();
            if action.operation == "update" {
                step.backend_phase = if reached { "reached" } else { "not_entered" }.into();
            }
            let capture_exhausted = outcome
                .as_ref()
                .err()
                .is_some_and(source_storage_witness::capture_exhausted);
            match outcome {
                Ok(value) => {
                    step.status = "completed".into();
                    step.result = value;
                }
                Err(e) => {
                    failed = true;
                    step.status = "failed".into();
                    step.failure = Some(failure(
                        "source_session_action_failed",
                        action.operation,
                        &e,
                    ));
                    deadline(at)?;
                    (step.after_state, step.after_state_failure) =
                        observation(&s, &input.created_at);
                }
            }
            w.after_action = Some(checked_checkpoint(&s, left, at)?);
            ensure!(
                !capture_exhausted,
                "source_session4_action_capture_exhausted"
            );
            // The only live source-state observation at this step; capture does not add one.
            match observed(&mut s, left, at) {
                Ok((value, typed)) => {
                    step.source_state = value;
                    if storage.current_snapshot.is_some() {
                        storage.current_source = Some(typed.clone());
                    }
                    w.source = Some(typed);
                }
                Err(e) => {
                    observation_failed = true;
                    step.source_state_failure = Some(failure(
                        "source_session_state_observation_failed",
                        "source_state",
                        &e,
                    ));
                    return Err(e);
                }
            }
            w.after_observation = Some(checked_checkpoint(&s, left, at)?);
            w.complete = true;
            observation_failed |= !step.resource_requests_complete;
            let kept_storage = if step.status == "completed"
                && matches!(action.operation, "serialize" | "diff")
                && w.turtle.is_none()
            {
                Some(&storage)
            } else {
                None
            };
            let n = source_storage_witness::size_until(&(&step, kept_storage, &w), left, at)?;
            retained += n;
            Ok(())
        })();
        if let Err(e) = capture {
            // Actual effects and reached phase remain; unretained values are explicitly unknown.
            step.revision_after = s.revision();
            if action.operation == "update" && !w.action_started {
                step.backend_phase = "not_entered".into();
            }
            let f = capture_failure(&e);
            step.status = "failed".into();
            step.result = Value::Null;
            if step.failure.is_none() {
                step.failure = Some(f.clone());
            }
            step.after_state = None;
            step.after_state_failure = None;
            step.source_state = Value::Null;
            step.source_state_failure = Some(f.clone());
            let started = w.action_started;
            w = empty_witness(action.id);
            w.action_started = started;
            budget_failure = Some(f);
            all_stopped = true;
        } else if step.status == "completed"
            && matches!(action.operation, "serialize" | "diff")
            && w.turtle.is_none()
        {
            storage_witnesses.push(storage);
        }
        steps.push(step);
        trace.steps.push(w);
    }
    let (mut final_state, mut final_state_failure) = match deadline(at) {
        Ok(()) => observation(&s, &input.created_at),
        Err(e) => (
            None,
            Some(failure(
                "source_session_state_observation_failed",
                "observe",
                &e,
            )),
        ),
    };
    trace.final_namespace = checkpoint(&s, limit.saturating_sub(retained), at);
    if let Err(e) = source_storage_witness::size_until(
        &(&final_state, &trace.final_namespace),
        limit.saturating_sub(retained),
        at,
    ) {
        let f = capture_failure(&e);
        final_state = None;
        final_state_failure = Some(f.clone());
        trace.final_namespace.status = "failed".into();
        trace.final_namespace.state = None;
        trace.final_namespace.failure = Some(f.clone());
        budget_failure = Some(f);
        all_stopped = true;
    }
    if trace.final_namespace.status != "captured" && budget_failure.is_none() {
        budget_failure = trace.final_namespace.failure.clone();
        all_stopped = true;
    }
    let status = if all_stopped {
        "partial"
    } else if failed || observation_failed || final_state_failure.is_some() {
        "failed"
    } else {
        "completed"
    };
    Ok(Result4 {
        schema: "semantica.source-session-result/4".into(),
        profile: "semantica.rdflib760-source-session/4".into(),
        request_sha256,
        source_bindings_sha256,
        initial_result: json!({"profile":"rdf","cache_queries":input.cache_queries}),
        initial_state,
        initial_state_failure,
        steps,
        status: status.into(),
        final_state,
        final_state_failure,
        failure: budget_failure,
        engineering_adoption: "not_assessed".into(),
        storage_witnesses,
        trace,
    })
}
impl crate::runtime::RustSemanticRuntime {
    pub fn execute_source_session4(&self, r: &Request4) -> Result<Result4, SemanticRuntimeFailure> {
        execute4(r)
    }
}
impl SemanticSourceSessionPortV4 for crate::runtime::RustSemanticRuntime {
    fn execute_source_session4(&self, r: &Request4) -> Result<Result4, SemanticRuntimeFailure> {
        execute4(r)
    }
}

fn check_checkpoint(c: &Checkpoint, at: Instant) -> Result<()> {
    ensure!(c.validate().is_ok(), "source_session4_checkpoint_contract");
    match c.status.as_str() {
        "captured" => {
            ensure!(
                c.failure.is_none() && c.retained_bytes.is_some() && c.retained_entries.is_some(),
                "source_session4_checkpoint_shape"
            );
            source_namespace_witness::validate_namespace_witness_until(
                c.state
                    .as_ref()
                    .context("source_session4_namespace_missing")?,
                MAX_RESULT_BYTES,
                at,
            )?;
        }
        "failed" => ensure!(
            c.state.is_none()
                && c.failure
                    .as_ref()
                    .is_some_and(|f| f.validate().is_ok() && f.phase == "capture")
                && c.retained_bytes.is_some()
                && c.retained_entries.is_some(),
            "source_session4_checkpoint_failure"
        ),
        "not_attempted" => ensure!(
            c.state.is_none()
                && c.failure.is_none()
                && c.retained_bytes.is_none()
                && c.retained_entries.is_none(),
            "source_session4_checkpoint_not_attempted"
        ),
        _ => bail!("source_session4_checkpoint_status"),
    }
    Ok(())
}
pub fn validate_relations4_until(input: &Request4, result: &Result4, at: Instant) -> Result<()> {
    deadline(at)?;
    validate_request4(input)?;
    ensure!(
        result.validate().is_ok()
            && result.schema == "semantica.source-session-result/4"
            && result.profile == input.profile
            && result.request_sha256 == request_sha256_4(input)?
            && result.source_bindings_sha256 == source_bindings_sha256_4(input)?,
        "source_session4_result_identity"
    );
    source_storage_witness::size_until(result, MAX_OUTPUT_BYTES, at)?;
    ensure!(
        result.steps.len() == input.actions.len() && result.trace.steps.len() == result.steps.len(),
        "source_session4_trace_coverage"
    );
    let trace = &result.trace;
    for c in [
        &trace.initial_before,
        &trace.initial_after,
        &trace.final_namespace,
    ] {
        check_checkpoint(c, at)?;
    }
    let initial_partial = result.initial_state_failure.is_some()
        && result.status == "partial"
        && result
            .failure
            .as_ref()
            .is_some_and(|f| f.phase == "capture");
    if initial_partial {
        ensure!(
            result.initial_state.is_null()
                && trace.initial_source.is_none()
                && trace.initial_before.status == "not_attempted"
                && trace.initial_after.status == "not_attempted",
            "source_session4_initial_partial"
        );
    } else {
        ensure!(
            trace.initial_before.status == "captured" && trace.initial_after.status == "captured",
            "source_session4_initial_namespace_missing"
        );
        source_storage_witness::check_state_observation(
            trace
                .initial_source
                .as_ref()
                .context("source_session4_initial_source_missing")?,
            &result.initial_state,
            at,
        )?;
    }
    let mut prior = if initial_partial {
        None
    } else {
        Some(&trace.initial_after)
    };
    let mut captured = source_storage_witness::size_until(
        &(
            &result.initial_state,
            &trace.initial_before,
            &trace.initial_after,
            &trace.initial_source,
        ),
        MAX_RESULT_BYTES,
        at,
    )?;
    let mut stopped = initial_partial;
    for ((a, step), w) in input.actions.iter().zip(&result.steps).zip(&trace.steps) {
        deadline(at)?;
        ensure!(
            w.validate().is_ok()
                && w.action_id == a.id
                && step.id == a.id
                && step.operation == a.operation,
            "source_session4_trace_identity"
        );
        if step.status == "skipped" {
            ensure!(
                !w.action_started
                    && !w.complete
                    && w.before.is_none()
                    && w.after_action.is_none()
                    && w.after_observation.is_none()
                    && w.source.is_none()
                    && w.turtle.is_none(),
                "source_session4_skipped_trace"
            );
            continue;
        }
        ensure!(!stopped, "source_session4_action_after_capture_stop");
        if !w.complete {
            ensure!(
                result.status == "partial"
                    && step.status == "failed"
                    && step
                        .source_state_failure
                        .as_ref()
                        .is_some_and(|f| f.phase == "capture"
                            && f.code == "source_session_result_byte_budget")
                    && w.before.is_none()
                    && w.after_action.is_none()
                    && w.after_observation.is_none()
                    && w.source.is_none()
                    && w.turtle.is_none(),
                "source_session4_incomplete_trace"
            );
            stopped = true;
            prior = None;
            continue;
        }
        ensure!(
            w.action_started,
            "source_session4_unstarted_completed_trace"
        );
        let before = w
            .before
            .as_ref()
            .context("source_session4_before_missing")?;
        let after = w
            .after_action
            .as_ref()
            .context("source_session4_after_missing")?;
        let observed = w
            .after_observation
            .as_ref()
            .context("source_session4_observed_missing")?;
        for c in [before, after, observed] {
            check_checkpoint(c, at)?;
            ensure!(
                c.status == "captured",
                "source_session4_complete_checkpoint"
            );
        }
        ensure!(Some(before) == prior, "source_session4_namespace_chain");
        let typed = w
            .source
            .as_ref()
            .context("source_session4_source_missing")?;
        source_storage_witness::check_state_observation(typed, &step.source_state, at)?;
        ensure!(
            step.source_state_failure.is_none(),
            "source_session4_source_ambiguous"
        );
        let turtle =
            a.operation == "serialize" && a.serialize_request.as_ref().is_some_and(is_turtle);
        if turtle && step.status == "completed" {
            let request = a.serialize_request.as_ref().unwrap();
            let text = step
                .result
                .as_str()
                .context("source_session4_turtle_text")?;
            let t = w
                .turtle
                .as_ref()
                .context("source_session4_turtle_missing")?;
            ensure!(
                step.revision_before == step.revision_after,
                "source_session4_turtle_revision"
            );
            ensure!(
                before.state.as_ref() == Some(&t.namespace_before)
                    && after.state.as_ref() == Some(&t.namespace_after)
                    && before.retained_bytes == Some(t.namespace_bytes_before)
                    && before.retained_entries == Some(t.namespace_entries_before)
                    && after.retained_bytes == Some(t.namespace_bytes_after)
                    && after.retained_entries == Some(t.namespace_entries_after),
                "source_session4_turtle_phase_binding"
            );
            source_turtle_transcript::validate_turtle_witness(
                request,
                text,
                t,
                &typed.quads,
                MAX_RESULT_BYTES,
                at,
            )?;
        } else {
            ensure!(w.turtle.is_none(), "source_session4_extraneous_turtle");
        }
        let storage = if step.status == "completed"
            && matches!(a.operation.as_str(), "serialize" | "diff")
            && w.turtle.is_none()
        {
            Some(
                result
                    .storage_witnesses
                    .iter()
                    .find(|v| v.action_id == a.id)
                    .context("source_session4_storage_missing")?,
            )
        } else {
            None
        };
        captured += source_storage_witness::size_until(
            &(step, storage, w),
            MAX_RESULT_BYTES.saturating_sub(captured),
            at,
        )?;
        prior = Some(observed);
    }
    if let Some(last) = prior {
        if trace.final_namespace.status == "captured" {
            ensure!(
                last == &trace.final_namespace,
                "source_session4_final_namespace_chain"
            );
        } else {
            ensure!(
                result.status == "partial" && result.failure.is_some(),
                "source_session4_final_capture_failure"
            );
        }
    }
    ensure!(
        result.status != "completed"
            || (!stopped && !initial_partial && trace.final_namespace.status == "captured"),
        "source_session4_incomplete_success"
    );
    if trace.final_namespace.status == "captured" {
        source_storage_witness::size_until(
            &(&result.final_state, &trace.final_namespace),
            MAX_RESULT_BYTES.saturating_sub(captured),
            at,
        )?;
    }
    validate_storage_relations_until(
        &input.actions,
        &input.created_at,
        &result.steps,
        &result.storage_witnesses,
        &result.status,
        at,
        true,
    )?;
    deadline(at)
}

#[cfg(test)]
#[path = "source_session4_receiving.rs"]
mod receiving;
