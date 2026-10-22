//! Shared original source-state observation; no comparison/oracle inputs.
//! Only presentation copies are sorted; live query rows are never reordered.
use super::DatasetSession;
use super::source_failure::SourceFailure;
use anyhow::{Context, Result};
use serde_json::{Value, json};
use spareval::{QueryEvaluationError, SourceNodeObservation, SourceTermInput};
use std::collections::BTreeMap;

/// The original state observer uses JSON sort_keys=True with the default
/// separators, and ensure_ascii=False for quads but True for contexts.
/// Scalar spelling remains owned by semantica-json. This only presents a copy.
pub(crate) fn observer_key(value: &Value, ensure_ascii: bool) -> Result<String, String> {
    fn string(text: &str, ascii: bool) -> Result<String, String> {
        let raw = semantica_json::canonical_json(&Value::String(text.into()))
            .map_err(|e| e.to_string())?;
        let raw = String::from_utf8(raw).map_err(|e| e.to_string())?;
        if !ascii {
            return Ok(raw);
        }
        let mut out = String::new();
        for c in raw.chars() {
            if c as u32 >= 0x7f {
                let mut units = [0u16; 2];
                for unit in c.encode_utf16(&mut units).iter() {
                    use std::fmt::Write;
                    write!(&mut out, "\\u{unit:04x}").map_err(|e| e.to_string())?;
                }
            } else {
                out.push(c);
            }
        }
        Ok(out)
    }
    match value {
        Value::String(v) => string(v, ensure_ascii),
        Value::Array(v) => Ok(format!(
            "[{}]",
            v.iter()
                .map(|v| observer_key(v, ensure_ascii))
                .collect::<Result<Vec<_>, _>>()?
                .join(", ")
        )),
        Value::Object(v) => {
            let sorted: BTreeMap<_, _> = v.iter().collect();
            let members = sorted
                .into_iter()
                .map(|(k, v)| {
                    Ok(format!(
                        "{}: {}",
                        string(k, ensure_ascii)?,
                        observer_key(v, ensure_ascii)?
                    ))
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(format!("{{{}}}", members.join(", ")))
        }
        _ => String::from_utf8(semantica_json::canonical_json(value).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string()),
    }
}

pub(crate) fn sort_observation(values: &mut [Value], ensure_ascii: bool) -> Result<(), String> {
    let mut keyed = values
        .iter()
        .map(|v| Ok((observer_key(v, ensure_ascii)?, v.clone())))
        .collect::<Result<Vec<_>, String>>()?;
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    for (target, (_, source)) in values.iter_mut().zip(keyed) {
        *target = source;
    }
    Ok(())
}
pub(crate) fn observed_node(
    value: SourceNodeObservation<'_>,
) -> std::result::Result<Value, QueryEvaluationError> {
    // runtime_dto converts the actual raw source object through the SAME owner
    // used by SELECT. No test branch manufactures the Variable DTO exception.
    present_node(value.term, value.runtime_dto())
}
/// Exact shared original observation presentation, including failed Variable
/// DTOs. Witness coverage must not replace an error observation with a fact.
pub(super) fn present_source_term(
    term: SourceTermInput<'_>,
) -> std::result::Result<Value, QueryEvaluationError> {
    present_node(term, SourceNodeObservation::present_source_term(term))
}
fn present_node(
    term: SourceTermInput<'_>,
    dto: std::result::Result<Value, QueryEvaluationError>,
) -> std::result::Result<Value, QueryEvaluationError> {
    match dto {
        Ok(dto) => Ok(dto),
        Err(error) => {
            let mapped = SourceFailure::query(error);
            let Some(failure) = mapped.downcast_ref::<SourceFailure>() else {
                return Err(QueryEvaluationError::Unexpected(
                    format!("source_observation_dto_unreceived: {mapped:#}").into(),
                ));
            };
            let (kind, text) = match term {
                SourceTermInput::Variable(v) => ("Variable", v),
                SourceTermInput::Iri(v) => ("URIRef", v),
                SourceTermInput::Blank(v) => ("BNode", v),
                SourceTermInput::Literal { value, .. } => ("Literal", value),
            };
            let node = super::source_update::node(term);
            Ok(
                json!({"backend_term":{"module":"rdflib.term","type":kind,"text":text,"n3":node.n3()},"dto_error":{"module":failure.module,"type":failure.class,"message":failure.to_string()}}),
            )
        }
    }
}
pub(crate) fn state(session: &mut DatasetSession) -> Result<Value> {
    state_captured(session, None).map(|v| v.0)
}
pub(super) fn state_captured(
    session: &mut DatasetSession,
    capture_limit: Option<usize>,
) -> Result<(
    Value,
    Option<semantica_contracts::SemanticSourceStateWitness3>,
)> {
    state_captured_until(session, capture_limit, None)
}
pub(super) fn state_captured_until(
    session: &mut DatasetSession,
    capture_limit: Option<usize>,
    at: Option<std::time::Instant>,
) -> Result<(
    Value,
    Option<semantica_contracts::SemanticSourceStateWitness3>,
)> {
    let check_time = || -> Result<(), QueryEvaluationError> {
        if at.is_some_and(|at| std::time::Instant::now() >= at) {
            return Err(QueryEvaluationError::Unexpected(
                "source_session_receiving_deadline".into(),
            ));
        }
        Ok(())
    };
    check_time()?;
    let mut typed_quads = Vec::new();
    let mut typed_contexts = Vec::new();
    let captured = std::cell::Cell::new(4usize);
    let mut quads = Vec::new();
    let mut contexts = Vec::new();
    let namespace_bytes = session.dataset_byte_limit();
    let namespace_entries = session.resource_profile.quads();
    let namespaces = &mut session.source_namespaces;
    session
        .rdflib_projection
        .source_state
        .as_mut()
        .context("source_state_missing")?
        .visit_observations_with_default_hook(
            |row| {
                check_time()?;
                if let Some(limit) = capture_limit {
                    let graph =
                        if matches!(row[3].term, SourceTermInput::Iri("urn:x-rdflib:default")) {
                            None
                        } else {
                            Some(row[3].term)
                        };
                    let typed = super::source_storage_witness::owned_row(
                        [row[0].term, row[1].term, row[2].term],
                        graph,
                    );
                    let bytes = super::source_storage_witness::size(
                        &typed,
                        limit.saturating_sub(captured.get()),
                    )
                    .map_err(|_| {
                        QueryEvaluationError::Unexpected(Box::new(
                            super::source_storage_witness::WitnessCaptureLimit,
                        ))
                    })?;
                    let count = captured
                        .get()
                        .checked_add(bytes + 1)
                        .filter(|n| *n <= limit)
                        .ok_or_else(|| {
                            QueryEvaluationError::Unexpected(Box::new(
                                super::source_storage_witness::WitnessCaptureLimit,
                            ))
                        })?;
                    captured.set(count);
                    typed_quads.push(typed);
                }
                let mut fields = Vec::with_capacity(4);
                for (position, value) in row.into_iter().enumerate() {
                    if position == 3
                        && matches!(value.term, SourceTermInput::Iri("urn:x-rdflib:default"))
                    {
                        fields.push(Value::Null);
                    } else {
                        fields.push(observed_node(value)?);
                    }
                }
                quads.push(Value::Array(fields));
                Ok(())
            },
            |value| {
                check_time()?;
                if let Some(limit) = capture_limit {
                    let typed = super::source_storage_witness::owned_term(value.term);
                    let bytes = super::source_storage_witness::size(
                        &typed,
                        limit.saturating_sub(captured.get()),
                    )
                    .map_err(|_| {
                        QueryEvaluationError::Unexpected(Box::new(
                            super::source_storage_witness::WitnessCaptureLimit,
                        ))
                    })?;
                    let count = captured
                        .get()
                        .checked_add(bytes + 1)
                        .filter(|n| *n <= limit)
                        .ok_or_else(|| {
                            QueryEvaluationError::Unexpected(Box::new(
                                super::source_storage_witness::WitnessCaptureLimit,
                            ))
                        })?;
                    captured.set(count);
                    typed_contexts.push(typed);
                }
                contexts.push(observed_node(value)?);
                Ok(())
            },
            || {
                check_time()?;
                namespaces
                    .initialize(true, namespace_bytes, namespace_entries)
                    .map_err(|error| QueryEvaluationError::Unexpected(error.into()))
            },
        )?;
    check_time()?;
    // Sort observation copies only, exactly at the original observer's point.
    sort_observation(&mut quads, false).map_err(anyhow::Error::msg)?;
    sort_observation(&mut contexts, true).map_err(anyhow::Error::msg)?;
    check_time()?;
    Ok((
        json!({"contexts":contexts,"revision":session.revision(),"query_cache_entries":session.query_cache.len(),"snapshot_cached":session.rdflib_snapshot_cache.get().is_some(),"quad_count":session.quad_count(),"quads":quads}),
        capture_limit.map(|_| semantica_contracts::SemanticSourceStateWitness3 {
            quads: typed_quads,
            contexts: typed_contexts,
        }),
    ))
}
