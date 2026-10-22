//! Explicit-clock transport of source BiTemporalFact and point graph queries.
//! Raw snapshots remain observations, never engineering adoption decisions.
use crate::{
    runtime::RustSemanticRuntime,
    temporal_facts::{date, list, object, scalar_id},
    temporal_intervals as time,
};
use chrono::{NaiveDate, NaiveDateTime};
use semantica_contracts::{
    SemanticBiTemporalFields, SemanticRuntimeFailure, SemanticTemporalQueryView,
    SemanticTemporalSnapshotPort, SemanticTemporalSnapshotRequest, SemanticTemporalSnapshotResult,
};
use serde_json::{Map, Value};
use std::collections::HashMap;

pub const MAX_INPUT_BYTES: usize = 1024 * 1024;
pub const MAX_OUTPUT_BYTES: usize = 16 * 1024 * 1024;
type Result<T> = std::result::Result<T, SemanticRuntimeFailure>;
fn invalid(message: impl ToString) -> SemanticRuntimeFailure {
    SemanticRuntimeFailure {
        code: "temporal_snapshot_invalid".into(),
        phase: "execution".into(),
        message: message.to_string(),
        retryable: false,
    }
}
struct Fact {
    start: Option<NaiveDateTime>,
    end: Option<NaiveDateTime>,
    recorded: NaiveDateTime,
    superseded: Option<NaiveDateTime>,
    basis: &'static str,
}
fn normalize(f: &Map<String, Value>, clock: NaiveDateTime, fallbacks: &mut u64) -> Result<Fact> {
    let start = date(f.get("valid_from"), false)?;
    let end = date(f.get("valid_until"), true)?;
    let (recorded, basis) = if let Some(recorded) = date(f.get("recorded_at"), false)? {
        (recorded, "explicit")
    } else if let Some(start) = start {
        (start, "valid_from")
    } else {
        *fallbacks += 1;
        (clock, "request_clock")
    };
    let superseded = date(f.get("superseded_at"), true)?;
    Ok(Fact {
        start,
        end,
        recorded,
        superseded,
        basis,
    })
}
pub(crate) fn axis_bounds(
    f: &Map<String, Value>,
    axis: &str,
    clock: NaiveDateTime,
    fallbacks: &mut u64,
) -> Result<(Option<NaiveDateTime>, Option<NaiveDateTime>)> {
    let f = normalize(f, clock, fallbacks)?;
    match axis {
        "valid" => Ok((f.start, f.end)),
        "transaction" => Ok((Some(f.recorded), f.superseded)),
        _ => Err(invalid("Unsupported time axis")),
    }
}
fn active(
    f: &Map<String, Value>,
    r: &SemanticTemporalSnapshotRequest,
    point: Option<NaiveDateTime>,
    clock: NaiveDateTime,
    fallbacks: &mut u64,
) -> Result<bool> {
    let axes: &[&str] = if r.time_axis == "both" {
        &["valid", "transaction"]
    } else {
        &[&r.time_axis]
    };
    for axis in axes {
        // The original normalizes each axis separately; retain clock calls and
        // short circuit timing. Rewrapping normalized fields needs no new clock.
        let (start, end) = axis_bounds(f, axis, clock, fallbacks)?;
        let interval = time::Interval {
            start: start.unwrap_or_else(|| {
                NaiveDate::from_ymd_opt(1, 1, 1)
                    .unwrap()
                    .and_hms_opt(0, 0, 0)
                    .unwrap()
            }),
            end,
            label: None,
        };
        let point =
            point.ok_or_else(|| invalid("Nonempty temporal graph requires a concrete point"))?;
        if !time::active_at(interval, point, Some(&r.granularity))? {
            return Ok(false);
        }
    }
    Ok(true)
}
fn reconstruct(
    r: &SemanticTemporalSnapshotRequest,
    point: Option<NaiveDateTime>,
    clock: NaiveDateTime,
    fallbacks: &mut u64,
) -> Result<(Vec<u64>, Vec<u64>)> {
    let graph = object(&r.graph)?;
    let entities = list(graph, "entities")?;
    let relationships = list(graph, "relationships")?;
    if entities.len().saturating_add(relationships.len()) > 4096 {
        return Err(invalid("More than4096 input records"));
    }
    let mut index = HashMap::<String, usize>::new();
    let mut entity_indices = Vec::new();
    let empty = Value::String(String::new());
    for (i, raw) in entities.iter().enumerate() {
        let entity = object(raw)?;
        if active(entity, r, point, clock, fallbacks)? {
            let key = scalar_id(
                entity
                    .get("id")
                    .unwrap_or_else(|| entity.get("name").unwrap_or(&empty)),
            )?;
            if let Some(position) = index.get(&key) {
                entity_indices[*position] = i as u64;
            } else {
                index.insert(key, entity_indices.len());
                entity_indices.push(i as u64);
            }
        }
    }
    let mut relationship_indices = Vec::new();
    for (i, raw) in relationships.iter().enumerate() {
        let relationship = object(raw)?;
        if !active(relationship, r, point, clock, fallbacks)? {
            continue;
        }
        // An actually empty input entity list has a distinct source branch.
        // Even a nonempty list whose members all expired still closes endpoints.
        if !entities.is_empty() {
            let source = scalar_id(relationship.get("source").unwrap_or(&Value::Null))?;
            let target = scalar_id(relationship.get("target").unwrap_or(&Value::Null))?;
            if !index.contains_key(&source) || !index.contains_key(&target) {
                continue;
            }
        }
        relationship_indices.push(i as u64);
    }
    Ok((entity_indices, relationship_indices))
}
impl SemanticTemporalSnapshotPort for RustSemanticRuntime {
    fn temporal_snapshot(
        &self,
        r: &SemanticTemporalSnapshotRequest,
    ) -> Result<SemanticTemporalSnapshotResult> {
        crate::datalog_port::check_serialized(r, MAX_INPUT_BYTES, false)
            .map_err(|_| invalid("Complete request exceeds1MiB"))?;
        if r.schema != "semantica.temporal-snapshot-request/1" {
            return Err(invalid("Unknown schema"));
        }
        let query_only_empty =
            r.query.is_empty() && !r.include_history && r.temporal_precision.is_none();
        let shape = match r.operation.as_str() {
            "normalize-fact" => {
                r.fact.is_object()
                    && r.graph.is_null()
                    && r.at_time.is_none()
                    && r.time_axis == "valid"
                    && r.granularity == "day"
                    && query_only_empty
            }
            "reconstruct" => r.fact.is_null() && r.graph.is_object() && query_only_empty,
            "query-at" => r.fact.is_null() && r.graph.is_object(),
            _ => false,
        };
        if !shape {
            return Err(invalid("Operation and supplied fields disagree"));
        }
        let clock = time::timestamp(&r.clock_at)?;
        let mut result = SemanticTemporalSnapshotResult {
            schema: "semantica.temporal-snapshot-result/1".into(),
            profile: "semantica.source-temporal-snapshot/1".into(),
            request_sha256: crate::sha256(
                semantica_contracts::json::canonical_json(
                    &serde_json::to_value(r).map_err(invalid)?,
                )
                .map_err(invalid)?,
            ),
            operation: r.operation.clone(),
            clock_at: time::text(clock),
            fact: Value::Null,
            normalized: None,
            recorded_at_basis: None,
            snapshot: Value::Null,
            query_result: None,
            entity_indices: vec![],
            relationship_indices: vec![],
            clock_fallback_count: 0,
        };
        if r.operation == "normalize-fact" {
            let f = normalize(object(&r.fact)?, clock, &mut result.clock_fallback_count)?;
            result.fact = r.fact.clone();
            result.recorded_at_basis = Some(f.basis.into());
            result.normalized = Some(SemanticBiTemporalFields {
                valid_from: f.start.map(time::text),
                valid_until: f.end.map(time::text),
                recorded_at: time::text(f.recorded),
                superseded_at: f.superseded.map(time::text),
            });
        } else {
            let point = match r.at_time.as_deref() {
                None | Some("OPEN") => None,
                Some(v) => Some(time::timestamp(v)?),
            };
            (result.entity_indices, result.relationship_indices) =
                reconstruct(r, point, clock, &mut result.clock_fallback_count)?;
            let graph = object(&r.graph)?;
            let entities = list(graph, "entities")?;
            let relationships = list(graph, "relationships")?;
            if r.operation == "query-at" && r.include_history {
                result.relationship_indices = (0..relationships.len() as u64).collect();
            }
            let selected_entities: Vec<Value> = result
                .entity_indices
                .iter()
                .map(|&i| entities[i as usize].clone())
                .collect();
            let selected_relationships: Vec<Value> = result
                .relationship_indices
                .iter()
                .map(|&i| relationships[i as usize].clone())
                .collect();
            if r.operation == "query-at" {
                result.query_result = Some(SemanticTemporalQueryView {
                    query: r.query.clone(),
                    at_time: point.map(time::text),
                    num_entities: selected_entities.len() as u64,
                    num_relationships: selected_relationships.len() as u64,
                    entities: selected_entities,
                    relationships: selected_relationships,
                });
            } else {
                let mut snapshot = graph.clone();
                snapshot.insert("entities".into(), Value::Array(selected_entities));
                snapshot.insert("relationships".into(), Value::Array(selected_relationships));
                result.snapshot = Value::Object(snapshot);
            }
        }
        crate::datalog_port::check_serialized(&result, MAX_OUTPUT_BYTES - 1, true)
            .map_err(|_| invalid("Complete output exceeds16MiB including newline"))?;
        Ok(result)
    }
}
