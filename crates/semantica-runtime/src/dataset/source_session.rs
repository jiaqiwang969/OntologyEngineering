//! Explicit local source-session admission over the existing DatasetSession.
//! No transport, filesystem, network, registry, Project or adoption authority.
use super::source_storage::{SourceSnapshotInput, source_snapshot_argument};
use super::source_update_loader::{SourceUpdateBytesLoader, SourceUpdateResource};
use super::{DatasetSession, RdfFormat, source_failure::SourceFailure};
use anyhow::{Context, Result, ensure};
use semantica_contracts::{
    SemanticRdfLibDatasetSnapshot, SemanticRuntimeFailure, SemanticSourceSerializeRequest,
    SemanticSourceSessionAction, SemanticSourceSessionAction2, SemanticSourceSessionBinding,
    SemanticSourceSessionFailure, SemanticSourceSessionPortV1, SemanticSourceSessionPortV2,
    SemanticSourceSessionPortV3, SemanticSourceSessionRequest, SemanticSourceSessionRequest2,
    SemanticSourceSessionRequest3, SemanticSourceSessionResource, SemanticSourceSessionResult,
    SemanticSourceSessionResult2, SemanticSourceSessionResult3, SemanticSourceSessionStep,
    SemanticSourceSessionStep2, SemanticSourceSessionStorageWitness3,
    SemanticSourceSnapshotArgument,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub const PROFILE: &str = "semantica.rdflib760-source-session/1";
pub const MAX_ACTIONS: usize = 32;
pub const MAX_DECODED_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_WIRE_BYTES: usize = 6 * MAX_DECODED_BYTES + 1024 * 1024;
pub const MAX_RESULT_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_OUTPUT_BYTES: usize = 64 * 1024 * 1024;
pub const DEADLINE_MS: u64 = 60_000;

// Borrowed version adapters, not new session or storage owners. Payload strings
// and retained result trees are never copied merely to select a protocol.
struct ActionView<'a> {
    id: &'a str,
    operation: &'a str,
    resource_id: &'a Option<String>,
    query: &'a Option<String>,
    namespaces: &'a BTreeMap<String, String>,
    bindings: &'a [SemanticSourceSessionBinding],
    base_uri: &'a Option<String>,
    graph_name: &'a Option<String>,
    use_cache: bool,
    serialize_request: Option<&'a SemanticSourceSerializeRequest>,
    before_snapshot_action_id: Option<&'a str>,
    after_snapshot_action_id: Option<&'a str>,
    contract_valid: bool,
}
impl<'a> ActionView<'a> {
    fn v1(a: &'a SemanticSourceSessionAction) -> Self {
        Self {
            id: &a.id,
            operation: &a.operation,
            resource_id: &a.resource_id,
            query: &a.query,
            namespaces: &a.namespaces,
            bindings: &a.bindings,
            base_uri: &a.base_uri,
            graph_name: &a.graph_name,
            use_cache: a.use_cache,
            serialize_request: None,
            before_snapshot_action_id: None,
            after_snapshot_action_id: None,
            contract_valid: a.validate().is_ok(),
        }
    }
    fn v2(a: &'a SemanticSourceSessionAction2) -> Self {
        Self {
            id: &a.id,
            operation: &a.operation,
            resource_id: &a.resource_id,
            query: &a.query,
            namespaces: &a.namespaces,
            bindings: &a.bindings,
            base_uri: &a.base_uri,
            graph_name: &a.graph_name,
            use_cache: a.use_cache,
            serialize_request: a.serialize_request.as_ref(),
            before_snapshot_action_id: a.before_snapshot_action_id.as_deref(),
            after_snapshot_action_id: a.after_snapshot_action_id.as_deref(),
            contract_valid: a.validate().is_ok(),
        }
    }
    fn storage_neutral(&self) -> bool {
        self.serialize_request.is_none()
            && self.before_snapshot_action_id.is_none()
            && self.after_snapshot_action_id.is_none()
    }
    fn common_neutral(&self) -> bool {
        self.query.is_none()
            && self.resource_id.is_none()
            && self.base_uri.is_none()
            && self.graph_name.is_none()
            && self.namespaces.is_empty()
            && self.bindings.is_empty()
            && !self.use_cache
    }
}
struct RequestView<'a> {
    schema: &'a str,
    profile: &'a str,
    created_at: &'a str,
    cache_queries: bool,
    resources: &'a [SemanticSourceSessionResource],
    actions: Vec<ActionView<'a>>,
}
impl<'a> RequestView<'a> {
    fn v1(r: &'a SemanticSourceSessionRequest) -> Self {
        Self {
            schema: &r.schema,
            profile: &r.profile,
            created_at: &r.created_at,
            cache_queries: r.cache_queries,
            resources: &r.resources,
            actions: r.actions.iter().map(ActionView::v1).collect(),
        }
    }
    fn v2(r: &'a SemanticSourceSessionRequest2) -> Self {
        Self {
            schema: &r.schema,
            profile: &r.profile,
            created_at: &r.created_at,
            cache_queries: r.cache_queries,
            resources: &r.resources,
            actions: r.actions.iter().map(ActionView::v2).collect(),
        }
    }
    fn v3(r: &'a SemanticSourceSessionRequest3) -> Self {
        Self {
            schema: &r.schema,
            profile: &r.profile,
            created_at: &r.created_at,
            cache_queries: r.cache_queries,
            resources: &r.resources,
            actions: r.actions.iter().map(ActionView::v2).collect(),
        }
    }
}

fn contract_failure(code: &str, message: impl ToString) -> SemanticRuntimeFailure {
    SemanticRuntimeFailure {
        code: code.into(),
        phase: "admission".into(),
        message: message.to_string(),
        retryable: false,
    }
}
fn failure(code: &str, phase: &str, error: &anyhow::Error) -> SemanticSourceSessionFailure {
    let original = error.downcast_ref::<SourceFailure>();
    SemanticSourceSessionFailure {
        code: code.into(),
        phase: phase.into(),
        message: format!("{error:#}"),
        original_module: original.map(|v| v.module.into()),
        original_class: original.map(|v| v.class.into()),
    }
}
fn format(value: &str) -> Result<RdfFormat> {
    Ok(match value {
        "turtle" => RdfFormat::Turtle,
        "trig" => RdfFormat::TriG,
        "ntriples" => RdfFormat::NTriples,
        "nquads" => RdfFormat::NQuads,
        "rdfxml" => RdfFormat::RdfXml,
        "jsonld" => RdfFormat::JsonLd {
            profile: Default::default(),
        },
        _ => anyhow::bail!("source_session_format_unsupported"),
    })
}
fn bindings(action: &ActionView<'_>) -> Result<BTreeMap<String, Value>> {
    let mut result = BTreeMap::new();
    for b in action.bindings {
        ensure!(b.validate().is_ok(), "source_session_binding_invalid");
        let value = json!({"term_type":b.term_type,"value":b.value,"datatype":b.datatype,"language":b.language});
        super::source_query::binding(&value)?;
        ensure!(
            result.insert(b.name.clone(), value).is_none(),
            "source_session_duplicate_binding"
        );
    }
    Ok(result)
}

pub(super) use super::source_storage_witness::size;

/// Entire request is checked before session creation or any action.
pub fn validate_request(input: &SemanticSourceSessionRequest) -> Result<()> {
    ensure!(input.validate().is_ok(), "source_session_contract_invalid");
    ensure!(
        !input.actions.is_empty() && input.actions.len() <= MAX_ACTIONS,
        "source_session_action_budget"
    );
    validate_common(&RequestView::v1(input))?;
    size(input, MAX_WIRE_BYTES)?;
    Ok(())
}
pub fn validate_request2(input: &SemanticSourceSessionRequest2) -> Result<()> {
    ensure!(input.validate().is_ok(), "source_session_contract_invalid");
    ensure!(
        !input.actions.is_empty() && input.actions.len() <= MAX_ACTIONS,
        "source_session_action_budget"
    );
    validate_common(&RequestView::v2(input))?;
    size(input, MAX_WIRE_BYTES)?;
    Ok(())
}
pub fn validate_request3(input: &SemanticSourceSessionRequest3) -> Result<()> {
    ensure!(input.validate().is_ok(), "source_session_request_invalid");
    ensure!(
        input.schema == "semantica.source-session-request/3"
            && input.profile == "semantica.rdflib760-source-session/3",
        "source_session_request_profile"
    );
    validate_common(&RequestView::v3(input))?;
    size(input, MAX_WIRE_BYTES)?;
    Ok(())
}
fn validate_common(input: &RequestView<'_>) -> Result<()> {
    ensure!(
        !input.actions.is_empty() && input.actions.len() <= MAX_ACTIONS,
        "source_session_action_budget"
    );
    ensure!(
        input.resources.len() <= 128,
        "source_session_resource_count_budget"
    );
    let mut decoded = 0usize;
    let mut add = |text: &str| -> Result<()> {
        decoded = decoded
            .checked_add(text.len())
            .context("source_session_decoded_byte_budget")?;
        ensure!(
            decoded <= MAX_DECODED_BYTES,
            "source_session_decoded_byte_budget"
        );
        Ok(())
    };
    for value in [input.schema, input.profile, input.created_at] {
        add(value)?;
    }
    let mut resource_ids = BTreeSet::new();
    let mut resource_bytes = 0usize;
    for resource in input.resources {
        ensure!(
            resource.validate().is_ok() && resource.binding.validate().is_ok(),
            "source_session_resource_invalid"
        );
        let b = &resource.binding;
        oxigraph::model::NamedNode::new(b.source_id.as_str())?;
        ensure!(
            resource_ids.insert(b.source_id.as_str()),
            "source_session_duplicate_resource"
        );
        format(&resource.rdf_format)?;
        ensure!(
            b.byte_count == resource.data.len() as u64
                && crate::sha256(resource.data.as_bytes()) == b.source_sha256,
            "source_session_source_identity_mismatch"
        );
        resource_bytes = resource_bytes
            .checked_add(resource.data.len())
            .context("source_session_resource_byte_budget")?;
        ensure!(
            resource_bytes <= crate::MAX_BYTES,
            "source_session_resource_byte_budget"
        );
        for value in [
            &resource.data,
            &resource.rdf_format,
            &b.schema,
            &b.source_id,
            &b.source_sha256,
        ] {
            add(value)?;
        }
    }
    let mut ids = BTreeSet::new();
    let mut snapshots = BTreeSet::new();
    for a in &input.actions {
        ensure!(
            a.contract_valid && ids.insert(a.id),
            "source_session_action_identity_invalid"
        );
        add(&a.id)?;
        add(&a.operation)?;
        for v in [a.resource_id, a.query, a.base_uri, a.graph_name]
            .into_iter()
            .flatten()
        {
            add(v)?;
        }
        ensure!(
            a.namespaces.len() <= 128 && a.bindings.len() <= 128,
            "source_session_parameter_count_budget"
        );
        for (k, v) in a.namespaces {
            add(k)?;
            add(v)?;
        }
        for b in a.bindings {
            for v in [&b.name, &b.term_type, &b.value] {
                add(v)?;
            }
            for v in [&b.datatype, &b.language].into_iter().flatten() {
                add(v)?;
            }
        }
        let bound = bindings(a)?;
        // Retain the original exact serialized options limit, not a decoded shortcut.
        ensure!(
            super::query::bounded_json_size(&(&a.namespaces, &bound))?.is_some(),
            "source_session_parameter_byte_budget"
        );
        if let Some(text) = &a.query {
            ensure!(
                text.len() <= 1024 * 1024,
                "source_session_query_byte_budget"
            );
        }
        for value in [a.before_snapshot_action_id, a.after_snapshot_action_id]
            .into_iter()
            .flatten()
        {
            add(value)?;
        }
        if let Some(storage) = a.serialize_request {
            ensure!(
                storage.validate().is_ok(),
                "source_session_serialize_contract_invalid"
            );
            add(&storage.format)?;
            for value in [&storage.graph_name, &storage.base_uri]
                .into_iter()
                .flatten()
            {
                add(value)?;
            }
        }
        if !matches!(a.operation, "serialize" | "diff") {
            ensure!(
                a.storage_neutral(),
                "source_session_unexpected_storage_arguments"
            );
        }
        match a.operation {
            "load" => ensure!(
                a.resource_id
                    .as_deref()
                    .is_some_and(|id| resource_ids.contains(id))
                    && a.query.is_none()
                    && a.namespaces.is_empty()
                    && a.bindings.is_empty(),
                "source_session_load_arguments"
            ),
            "clear" => ensure!(
                a.query.is_none()
                    && a.resource_id.is_none()
                    && a.base_uri.is_none()
                    && a.namespaces.is_empty()
                    && a.bindings.is_empty(),
                "source_session_clear_arguments"
            ),
            "query" | "update" => ensure!(
                a.query.is_some()
                    && a.resource_id.is_none()
                    && a.base_uri.is_none()
                    && a.graph_name.is_none(),
                "source_session_query_arguments"
            ),
            "snapshot" => ensure!(
                a.query.is_none()
                    && a.resource_id.is_none()
                    && a.base_uri.is_none()
                    && a.graph_name.is_none()
                    && a.namespaces.is_empty()
                    && a.bindings.is_empty(),
                "source_session_snapshot_arguments"
            ),
            "serialize" => ensure!(
                a.common_neutral()
                    && a.serialize_request.is_some()
                    && a.before_snapshot_action_id.is_none()
                    && a.after_snapshot_action_id.is_none(),
                "source_session_serialize_arguments"
            ),
            "diff" => ensure!(
                a.common_neutral()
                    && a.serialize_request.is_none()
                    && a.before_snapshot_action_id
                        .is_some_and(|id| snapshots.contains(id))
                    && a.after_snapshot_action_id
                        .is_none_or(|id| snapshots.contains(id)),
                "source_session_diff_reference_not_prior_snapshot"
            ),
            _ => anyhow::bail!("source_session_operation_unsupported"),
        }
        if a.operation == "snapshot" {
            snapshots.insert(a.id);
        }
    }
    Ok(())
}

pub fn source_bindings_sha256(input: &SemanticSourceSessionRequest) -> Result<String> {
    let bindings: Vec<_> = input.resources.iter().map(|r| &r.binding).collect();
    Ok(crate::sha256(
        semantica_json::ordered_json(&serde_json::to_value(bindings)?)?.as_slice(),
    ))
}

pub fn request_sha256(input: &SemanticSourceSessionRequest) -> Result<String> {
    // Caller validates the complete bounded DTO before this canonical allocation.
    Ok(crate::sha256(
        semantica_json::ordered_json(&serde_json::to_value(input)?)?.as_slice(),
    ))
}
pub fn source_bindings_sha256_2(input: &SemanticSourceSessionRequest2) -> Result<String> {
    let bindings: Vec<_> = input.resources.iter().map(|r| &r.binding).collect();
    Ok(crate::sha256(
        semantica_json::ordered_json(&serde_json::to_value(bindings)?)?.as_slice(),
    ))
}
pub fn request_sha256_2(input: &SemanticSourceSessionRequest2) -> Result<String> {
    Ok(crate::sha256(
        semantica_json::ordered_json(&serde_json::to_value(input)?)?.as_slice(),
    ))
}
pub fn source_bindings_sha256_3(input: &SemanticSourceSessionRequest3) -> Result<String> {
    let bindings: Vec<_> = input.resources.iter().map(|r| &r.binding).collect();
    Ok(crate::sha256(&semantica_json::ordered_json(
        &serde_json::to_value(bindings)?,
    )?))
}
pub fn request_sha256_3(input: &SemanticSourceSessionRequest3) -> Result<String> {
    Ok(crate::sha256(&semantica_json::ordered_json(
        &serde_json::to_value(input)?,
    )?))
}
fn retained_snapshot(
    steps: &[SemanticSourceSessionStep2],
    id: &str,
) -> Result<SemanticSourceSnapshotArgument> {
    let step = steps
        .iter()
        .find(|s| s.id == id)
        .context("source_session_diff_snapshot_missing")?;
    ensure!(
        step.operation == "snapshot"
            && step.status == "completed"
            && step.failure.is_none()
            && !step.result.is_null(),
        "source_session_diff_snapshot_not_retained"
    );
    let snapshot: SemanticRdfLibDatasetSnapshot = serde::Deserialize::deserialize(&step.result)?;
    ensure!(
        snapshot.validate().is_ok(),
        "source_session_diff_snapshot_invalid"
    );
    Ok(source_snapshot_argument(&snapshot))
}

fn observation(
    session: &DatasetSession,
    at: &str,
) -> (
    Option<SemanticRdfLibDatasetSnapshot>,
    Option<SemanticSourceSessionFailure>,
) {
    // Snapshot caching is confined to this disposable readback copy. Live cache
    // state just invalidated by a backend failure must remain invalidated.
    let result = session.clone().snapshot_rdflib(at).and_then(|value| {
        size(&value, crate::MAX_BYTES)?;
        Ok(value)
    });
    match result {
        Ok(v) => (Some(v), None),
        Err(e) => (
            None,
            Some(failure(
                "source_session_state_observation_failed",
                "observe",
                &e,
            )),
        ),
    }
}
struct ResourceLoader<'a> {
    inner: SourceUpdateBytesLoader<'a>,
    requests: Vec<String>,
    bytes: usize,
    complete: bool,
}
impl spareval::SourceUpdateLoader for ResourceLoader<'_> {
    fn load(
        &mut self,
        iri: &str,
        emit: &mut dyn FnMut(
            [spareval::SourceTermInput<'_>; 3],
        ) -> Result<(), spareval::QueryEvaluationError>,
    ) -> Result<bool, spareval::QueryEvaluationError> {
        if iri.len() <= crate::MAX_BYTES.saturating_sub(self.bytes) {
            self.bytes += iri.len();
            self.requests.push(iri.into());
        } else {
            self.complete = false;
        }
        // Observation capacity never grants authority or changes execution.
        spareval::SourceUpdateLoader::load(&mut self.inner, iri, emit)
    }
}
fn loader(resources: &[SemanticSourceSessionResource]) -> Result<ResourceLoader<'_>> {
    // Formats other than Turtle may be explicitly loaded, but have no implicit
    // LOAD/USING permission. The existing loader's syntax remains unchanged.
    let resources = resources
        .iter()
        .filter(|r| r.rdf_format == "turtle")
        .map(|r| {
            (
                r.binding.source_id.clone(),
                SourceUpdateResource::Turtle {
                    bytes: r.data.as_bytes(),
                    sha256: &r.binding.source_sha256,
                },
            )
        })
        .collect();
    Ok(ResourceLoader {
        inner: SourceUpdateBytesLoader::new(resources, crate::MAX_BYTES, super::MAX_QUADS)?,
        requests: Vec::new(),
        bytes: 0,
        complete: true,
    })
}

fn state(session: &mut DatasetSession) -> (Value, Option<SemanticSourceSessionFailure>) {
    match super::source_observation::state(session).and_then(|v| {
        size(&v, crate::MAX_BYTES)?;
        Ok(v)
    }) {
        Ok(v) => (v, None),
        Err(e) => (
            Value::Null,
            Some(failure(
                "source_session_state_observation_failed",
                "state",
                &e,
            )),
        ),
    }
}

fn execute(
    input: &SemanticSourceSessionRequest,
) -> Result<SemanticSourceSessionResult, SemanticRuntimeFailure> {
    validate_request(input).map_err(|e| contract_failure("source_session_admission_refused", e))?;
    let request_sha256 = request_sha256(input)
        .map_err(|e| contract_failure("source_session_request_identity_failed", e))?;
    let source_bindings_sha256 = source_bindings_sha256(input)
        .map_err(|e| contract_failure("source_session_source_identity_failed", e))?;
    let result = execute_shared(
        &RequestView::v1(input),
        request_sha256,
        source_bindings_sha256,
        false,
    )
    .0;
    Ok(SemanticSourceSessionResult {
        schema: "semantica.source-session-result/1".into(),
        profile: PROFILE.into(),
        request_sha256: result.request_sha256,
        source_bindings_sha256: result.source_bindings_sha256,
        initial_result: result.initial_result,
        initial_state: result.initial_state,
        initial_state_failure: result.initial_state_failure,
        status: result.status,
        final_state: result.final_state,
        final_state_failure: result.final_state_failure,
        failure: result.failure,
        engineering_adoption: result.engineering_adoption,
        steps: result
            .steps
            .into_iter()
            .map(|step| SemanticSourceSessionStep {
                id: step.id,
                operation: step.operation,
                status: step.status,
                backend_phase: step.backend_phase,
                revision_before: step.revision_before,
                revision_after: step.revision_after,
                result: step.result,
                failure: step.failure,
                after_state: step.after_state,
                after_state_failure: step.after_state_failure,
                source_state: step.source_state,
                source_state_failure: step.source_state_failure,
                resource_requests: step.resource_requests,
                resource_requests_complete: step.resource_requests_complete,
            })
            .collect(),
    })
}
fn execute2(
    input: &SemanticSourceSessionRequest2,
) -> Result<SemanticSourceSessionResult2, SemanticRuntimeFailure> {
    validate_request2(input)
        .map_err(|e| contract_failure("source_session_admission_refused", e))?;
    let request = request_sha256_2(input)
        .map_err(|e| contract_failure("source_session_request_identity_failed", e))?;
    let sources = source_bindings_sha256_2(input)
        .map_err(|e| contract_failure("source_session_source_identity_failed", e))?;
    Ok(execute_shared(&RequestView::v2(input), request, sources, false).0)
}

fn is_turtle(r: &SemanticSourceSerializeRequest) -> bool {
    matches!(
        semantica_json::python_strip(&r.format)
            .to_lowercase()
            .as_str(),
        "turtle" | "ttl"
    )
}
fn execute_action(
    session: &mut DatasetSession,
    input: &RequestView<'_>,
    action: &ActionView<'_>,
    steps: &[SemanticSourceSessionStep2],
    step: &mut SemanticSourceSessionStep2,
    witness: &mut SemanticSourceSessionStorageWitness3,
    capture_witnesses: bool,
    retained: usize,
    reached: &mut bool,
    mut turtle: Option<&mut Option<semantica_contracts::SemanticSourceTurtleWitness4>>,
    deadline: Option<std::time::Instant>,
) -> Result<Value> {
    Ok(match action.operation {
        "load" => {
            let resource = input
                .resources
                .iter()
                .find(|r| Some(&r.binding.source_id) == action.resource_id.as_ref())
                .context("source_session_resource_missing")?;
            serde_json::to_value(session.load_source_internal(
                resource.data.as_bytes(),
                format(&resource.rdf_format)?,
                action.base_uri.as_deref(),
                action.graph_name.as_deref(),
            )?)?
        }
        "query" => serde_json::to_value(session.query_source_internal(
            action.query.as_deref().unwrap(),
            &action.namespaces,
            &bindings(action)?,
            action.use_cache,
            None,
        )?)?,
        "update" => {
            let mut resources = loader(&input.resources)?;
            let result = session.update_source_full_observed_internal(
                &Value::String(action.query.as_ref().unwrap().clone()),
                &action.namespaces,
                &bindings(action)?,
                &mut resources,
                None,
                reached,
            );
            step.resource_requests = resources.requests;
            step.resource_requests_complete = resources.complete;
            serde_json::to_value(result?)?
        }
        "snapshot" => serde_json::to_value(session.snapshot_rdflib(&input.created_at)?)?,
        "clear" => serde_json::to_value(session.clear_rdflib(action.graph_name.as_deref())?)?,
        "serialize" => {
            let request = action
                .serialize_request
                .expect("validated serialize request");
            if let Some(target) = turtle.as_mut()
                && is_turtle(request)
            {
                let (text, actual) = session.capture_turtle(
                    request,
                    MAX_RESULT_BYTES.saturating_sub(retained),
                    deadline.expect("explicit4 deadline"),
                )?;
                **target = Some(actual);
                return Ok(Value::String(text));
            }
            if capture_witnesses {
                let (value, rows) = session.serialize_source_observed_internal(
                    request,
                    Some(MAX_RESULT_BYTES.saturating_sub(retained)),
                )?;
                witness.serialization = rows;
                serde_json::to_value(value)?
            } else {
                serde_json::to_value(session.serialize_source_nt_nq_internal(request)?)?
            }
        }
        "diff" => {
            let before = retained_snapshot(
                &steps,
                action
                    .before_snapshot_action_id
                    .expect("validated before reference"),
            )?;
            let after = action
                .after_snapshot_action_id
                .map(|id| retained_snapshot(&steps, id))
                .transpose()?;
            if capture_witnesses {
                let (value, current) = session.diff_source_observed_internal(
                    SourceSnapshotInput::Snapshot(&before),
                    after.as_ref().map(SourceSnapshotInput::Snapshot),
                    input.created_at,
                )?;
                witness.current_snapshot = current;
                serde_json::to_value(value)?
            } else {
                serde_json::to_value(session.diff_source_internal(
                    SourceSnapshotInput::Snapshot(&before),
                    after.as_ref().map(SourceSnapshotInput::Snapshot),
                    input.created_at,
                )?)?
            }
        }
        _ => unreachable!("validated operation"),
    })
}

fn execute_shared(
    input: &RequestView<'_>,
    request_sha256: String,
    source_bindings_sha256: String,
    capture_witnesses: bool,
) -> (
    SemanticSourceSessionResult2,
    Vec<SemanticSourceSessionStorageWitness3>,
) {
    let mut session = DatasetSession::new_source_update_internal(input.cache_queries);
    let (initial_state, initial_state_failure) = state(&mut session);
    let mut observation_failed = initial_state_failure.is_some();
    let mut steps = Vec::new();
    let mut storage_witnesses = Vec::new();
    let mut mutation_failed = false;
    let mut all_stopped = false;
    let mut retained = 0usize;
    let mut budget_failure = None;
    for action in &input.actions {
        let before = session.revision();
        let mut step = SemanticSourceSessionStep2 {
            id: action.id.to_owned(),
            operation: action.operation.to_owned(),
            status: "skipped".into(),
            backend_phase: "not_applicable".into(),
            revision_before: before,
            revision_after: before,
            result: Value::Null,
            failure: None,
            after_state: None,
            after_state_failure: None,
            source_state: Value::Null,
            source_state_failure: None,
            resource_requests: Vec::new(),
            resource_requests_complete: true,
        };
        if all_stopped
            || (mutation_failed && matches!(action.operation, "load" | "update" | "clear"))
        {
            steps.push(step);
            continue;
        }
        let mut witness = SemanticSourceSessionStorageWitness3 {
            action_id: action.id.into(),
            serialization: None,
            current_snapshot: None,
            current_source: None,
        };
        let mut reached = false;
        let outcome = execute_action(
            &mut session,
            input,
            action,
            &steps,
            &mut step,
            &mut witness,
            capture_witnesses,
            retained,
            &mut reached,
            None,
            None,
        );
        step.revision_after = session.revision();
        if action.operation == "update" {
            step.backend_phase = if reached { "reached" } else { "not_entered" }.into();
        }
        let mut witness_capture_failed = outcome
            .as_ref()
            .err()
            .is_some_and(super::source_storage_witness::capture_exhausted);
        match outcome {
            Ok(value) => {
                step.status = "completed".into();
                step.result = value;
            }
            Err(error) => {
                mutation_failed = true;
                step.status = "failed".into();
                step.failure = Some(failure(
                    "source_session_action_failed",
                    &action.operation,
                    &error,
                ));
                (step.after_state, step.after_state_failure) =
                    observation(&session, &input.created_at);
            }
        }
        if capture_witnesses && step.status == "completed" && witness.current_snapshot.is_some() {
            match super::source_observation::state_captured(
                &mut session,
                Some(MAX_RESULT_BYTES.saturating_sub(retained)),
            ) {
                Ok((value, typed)) => match size(&value, 16 * 1024 * 1024) {
                    Ok(_) => {
                        step.source_state = value;
                        witness.current_source = typed;
                    }
                    Err(error) => {
                        step.source_state_failure = Some(failure(
                            "source_session_state_observation_failed",
                            "source_state",
                            &error,
                        ));
                    }
                },
                Err(error) => {
                    witness_capture_failed |=
                        super::source_storage_witness::capture_exhausted(&error);
                    step.source_state_failure = Some(failure(
                        "source_session_state_observation_failed",
                        "source_state",
                        &error,
                    ));
                }
            }
        } else {
            (step.source_state, step.source_state_failure) = state(&mut session);
        }
        observation_failed |=
            step.source_state_failure.is_some() || !step.resource_requests_complete;
        let storage_completed = capture_witnesses
            && step.status == "completed"
            && matches!(action.operation, "serialize" | "diff");
        let captured_size = if witness_capture_failed {
            Err(anyhow::anyhow!(
                super::source_storage_witness::WitnessCaptureLimit
            ))
        } else if storage_completed {
            size(
                &(&step, &witness),
                MAX_RESULT_BYTES.saturating_sub(retained),
            )
        } else {
            size(&step, MAX_RESULT_BYTES.saturating_sub(retained))
        };
        match captured_size {
            Ok(bytes) => {
                retained += bytes;
            }
            Err(error) => {
                // The operation may have completed. Its unretained result is
                // unknown; never claim a truncated value or roll the state back.
                step.status = "failed".into();
                step.result = Value::Null;
                step.after_state = None;
                step.after_state_failure = None;
                step.source_state = Value::Null;
                let e = failure("source_session_result_byte_budget", "capture", &error);
                if step.failure.is_none() {
                    step.failure = Some(e.clone());
                }
                step.source_state_failure = Some(e.clone());
                budget_failure = Some(e);
                all_stopped = true;
            }
        }
        if storage_completed && !all_stopped {
            storage_witnesses.push(witness);
        }
        steps.push(step);
    }
    let (final_state, final_state_failure) = observation(&session, &input.created_at);
    let status = if all_stopped {
        "partial"
    } else if mutation_failed || observation_failed || final_state_failure.is_some() {
        "failed"
    } else {
        "completed"
    };
    (
        SemanticSourceSessionResult2 {
            schema: "semantica.source-session-result/2".into(),
            profile: "semantica.rdflib760-source-session/2".into(),
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
        },
        storage_witnesses,
    )
}

impl crate::runtime::RustSemanticRuntime {
    pub fn execute_source_session(
        &self,
        input: &SemanticSourceSessionRequest,
    ) -> Result<SemanticSourceSessionResult, SemanticRuntimeFailure> {
        execute(input)
    }
}
impl SemanticSourceSessionPortV1 for crate::runtime::RustSemanticRuntime {
    fn execute_source_session(
        &self,
        input: &SemanticSourceSessionRequest,
    ) -> Result<SemanticSourceSessionResult, SemanticRuntimeFailure> {
        self.execute_source_session(input)
    }
}

impl crate::runtime::RustSemanticRuntime {
    pub fn execute_source_session2(
        &self,
        input: &SemanticSourceSessionRequest2,
    ) -> Result<SemanticSourceSessionResult2, SemanticRuntimeFailure> {
        execute2(input)
    }
}
impl SemanticSourceSessionPortV2 for crate::runtime::RustSemanticRuntime {
    fn execute_source_session2(
        &self,
        input: &SemanticSourceSessionRequest2,
    ) -> Result<SemanticSourceSessionResult2, SemanticRuntimeFailure> {
        self.execute_source_session2(input)
    }
}

fn execute3(
    input: &SemanticSourceSessionRequest3,
) -> Result<SemanticSourceSessionResult3, SemanticRuntimeFailure> {
    validate_request3(input)
        .map_err(|e| contract_failure("source_session_admission_refused", e))?;
    let request = request_sha256_3(input)
        .map_err(|e| contract_failure("source_session_request_identity_failed", e))?;
    let sources = source_bindings_sha256_3(input)
        .map_err(|e| contract_failure("source_session_source_identity_failed", e))?;
    let (body, storage_witnesses) = execute_shared(&RequestView::v3(input), request, sources, true);
    Ok(SemanticSourceSessionResult3 {
        schema: "semantica.source-session-result/3".into(),
        profile: "semantica.rdflib760-source-session/3".into(),
        request_sha256: body.request_sha256,
        source_bindings_sha256: body.source_bindings_sha256,
        initial_result: body.initial_result,
        initial_state: body.initial_state,
        initial_state_failure: body.initial_state_failure,
        steps: body.steps,
        status: body.status,
        final_state: body.final_state,
        final_state_failure: body.final_state_failure,
        failure: body.failure,
        engineering_adoption: body.engineering_adoption,
        storage_witnesses,
    })
}
impl crate::runtime::RustSemanticRuntime {
    pub fn execute_source_session3(
        &self,
        input: &SemanticSourceSessionRequest3,
    ) -> Result<SemanticSourceSessionResult3, SemanticRuntimeFailure> {
        execute3(input)
    }
}
impl SemanticSourceSessionPortV3 for crate::runtime::RustSemanticRuntime {
    fn execute_source_session3(
        &self,
        input: &SemanticSourceSessionRequest3,
    ) -> Result<SemanticSourceSessionResult3, SemanticRuntimeFailure> {
        self.execute_source_session3(input)
    }
}

/// Neutral relation checks share the actual source storage implementation.
/// Host execution/raw evidence authenticates provenance; witnesses do not.
pub fn validate_storage_relations3(
    input: &SemanticSourceSessionRequest3,
    result: &SemanticSourceSessionResult3,
) -> Result<()> {
    // Standalone receiving gets its own explicit bounded budget. Production
    // transport must instead pass its already-running absolute deadline below.
    validate_storage_relations3_until(
        input,
        result,
        std::time::Instant::now() + std::time::Duration::from_millis(DEADLINE_MS),
    )
}

/// Consume the caller's existing absolute deadline; never start a new host budget.
pub fn validate_storage_relations3_until(
    input: &SemanticSourceSessionRequest3,
    result: &SemanticSourceSessionResult3,
    deadline: std::time::Instant,
) -> Result<()> {
    use std::time::Instant;
    ensure!(
        Instant::now() < deadline,
        "source_session_receiving_deadline"
    );
    size(result, MAX_OUTPUT_BYTES)?;
    ensure!(
        Instant::now() < deadline,
        "source_session_receiving_deadline"
    );
    validate_storage_relations_until(
        &input.actions,
        &input.created_at,
        &result.steps,
        &result.storage_witnesses,
        &result.status,
        deadline,
        false,
    )
}
fn validate_storage_relations_until(
    actions: &[SemanticSourceSessionAction2],
    created_at: &str,
    steps: &[SemanticSourceSessionStep2],
    storage_witnesses: &[SemanticSourceSessionStorageWitness3],
    status: &str,
    deadline: std::time::Instant,
    turtle4: bool,
) -> Result<()> {
    use std::time::Instant;
    ensure!(
        actions.len() == steps.len(),
        "source_session_worker_action_coverage"
    );
    let mut witnesses = storage_witnesses.iter();
    let mut captured = 0usize;
    let mut capture_stopped = false;
    for (index, (action, step)) in actions.iter().zip(steps).enumerate() {
        ensure!(
            Instant::now() < deadline,
            "source_session_receiving_deadline"
        );
        let storage = step.status == "completed"
            && matches!(action.operation.as_str(), "serialize" | "diff")
            && !(turtle4
                && action.operation == "serialize"
                && action.serialize_request.as_ref().is_some_and(is_turtle));
        if !storage {
            capture_stopped |= step.source_state_failure.as_ref().is_some_and(|f| {
                f.phase == "capture" && f.code == "source_session_result_byte_budget"
            });
            if !capture_stopped && step.status != "skipped" {
                captured += size(step, MAX_RESULT_BYTES.saturating_sub(captured))?;
            }
            continue;
        }
        let witness = witnesses
            .next()
            .context("source_session_storage_witness_missing")?;
        ensure!(
            witness.action_id == action.id && witness.validate().is_ok(),
            "source_session_storage_witness_identity"
        );
        captured += size(&(step, witness), MAX_RESULT_BYTES.saturating_sub(captured))?;
        ensure!(
            step.revision_before == step.revision_after,
            "source_session_worker_storage_revision"
        );
        let observed = if step.source_state_failure.is_none() {
            Some(&step.source_state)
        } else {
            ensure!(
                status != "completed" && step.source_state.is_null(),
                "source_session_storage_state_unavailable"
            );
            None
        };
        if action.operation == "serialize" {
            ensure!(
                witness.current_snapshot.is_none() && witness.current_source.is_none(),
                "source_session_storage_extraneous_snapshot"
            );
            let writer = witness
                .serialization
                .as_ref()
                .context("source_session_writer_witness_missing")?;
            let value: semantica_contracts::SemanticSourceSerialization =
                serde_json::from_value(step.result.clone())?;
            ensure!(
                value.validate().is_ok(),
                "source_session_worker_serialize_invalid"
            );
            super::source_storage_witness::check_serialization(
                action
                    .serialize_request
                    .as_ref()
                    .context("source_session_serialize_request")?,
                writer,
                &value,
                observed,
                deadline,
            )?;
        } else {
            ensure!(
                witness.serialization.is_none(),
                "source_session_storage_extraneous_writer"
            );
            let endpoint = |id: &str| -> Result<SemanticRdfLibDatasetSnapshot> {
                let prior = steps[..index]
                    .iter()
                    .find(|s| s.id == id)
                    .context("source_session_worker_diff_endpoint_missing")?;
                ensure!(
                    prior.operation == "snapshot"
                        && prior.status == "completed"
                        && prior.failure.is_none(),
                    "source_session_worker_diff_endpoint_not_retained"
                );
                let v: SemanticRdfLibDatasetSnapshot =
                    serde_json::from_value(prior.result.clone())?;
                ensure!(
                    v.validate().is_ok(),
                    "source_session_worker_diff_endpoint_invalid"
                );
                Ok(v)
            };
            let before = endpoint(
                action
                    .before_snapshot_action_id
                    .as_deref()
                    .context("source_session_before_snapshot")?,
            )?;
            let after = if let Some(id) = &action.after_snapshot_action_id {
                ensure!(
                    witness.current_snapshot.is_none() && witness.current_source.is_none(),
                    "source_session_diff_extraneous_current"
                );
                endpoint(id)?
            } else {
                let v = witness
                    .current_snapshot
                    .as_ref()
                    .context("source_session_diff_current_witness_missing")?;
                ensure!(
                    v.validate().is_ok() && v.revision == step.revision_after,
                    "source_session_diff_current_identity"
                );
                // Time is part of the actual implicit endpoint even if a
                // subsequent source-state observation failed.
                super::source_storage_witness::check_snapshot_time(v, created_at)?;
                // This is the actual snapshot already taken by the diff owner,
                // not an extra observation or revision-only graph inference.
                if let Some(state) = observed {
                    ensure!(
                        state["snapshot_cached"] == true,
                        "source_session_diff_current_state"
                    );
                    let typed = witness
                        .current_source
                        .as_ref()
                        .context("source_session_diff_current_source_missing")?;
                    super::source_storage_witness::check_current_snapshot(
                        typed, state, v, deadline,
                    )?;
                } else {
                    ensure!(
                        witness.current_source.is_none(),
                        "source_session_diff_unobserved_state"
                    );
                }
                v.clone()
            };
            let value: semantica_contracts::SemanticSourceDatasetDiff =
                serde_json::from_value(step.result.clone())?;
            ensure!(
                value.validate().is_ok(),
                "source_session_worker_diff_invalid"
            );
            super::source_storage_witness::check_diff(
                &before, &after, &value, created_at, deadline,
            )?;
        }
    }
    ensure!(
        witnesses.next().is_none(),
        "source_session_storage_witness_extra"
    );
    ensure!(
        Instant::now() < deadline,
        "source_session_receiving_deadline"
    );
    Ok(())
}

#[path = "source_session4.rs"]
mod public4;
pub use public4::{
    request_sha256_4, source_bindings_sha256_4, validate_relations4_until, validate_request4,
};
