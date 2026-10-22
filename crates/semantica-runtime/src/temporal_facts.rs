//! Source timeline and revision classification. Raw records are preserved;
//! temporal overlap alone neither invalidates nor adopts an engineering decision.
use crate::{runtime::RustSemanticRuntime, temporal_intervals as time};
use chrono::{NaiveDate, NaiveDateTime};
use semantica_contracts::{
    SemanticRuntimeFailure, SemanticTemporalFactEvent, SemanticTemporalFactReference,
    SemanticTemporalFactsPort, SemanticTemporalFactsRequest, SemanticTemporalFactsResult,
};
use serde_json::{Map, Value};

pub const MAX_INPUT_BYTES: usize = 1024 * 1024;
pub const MAX_OUTPUT_BYTES: usize = 16 * 1024 * 1024;
type Result<T> = std::result::Result<T, SemanticRuntimeFailure>;
fn invalid(message: impl ToString) -> SemanticRuntimeFailure {
    SemanticRuntimeFailure {
        code: "temporal_facts_invalid".into(),
        phase: "execution".into(),
        message: message.to_string(),
        retryable: false,
    }
}
pub(crate) fn object(value: &Value) -> Result<&Map<String, Value>> {
    value
        .as_object()
        .ok_or_else(|| invalid("Expected a complete JSON dictionary"))
}
pub(crate) fn scalar_id(value: &Value) -> Result<String> {
    if value.is_array() || value.is_object() {
        return Err(invalid(
            "Complex identifiers are outside the scalar transport profile",
        ));
    }
    semantica_json::python_str(value).map_err(invalid)
}
pub(crate) fn date(value: Option<&Value>, bound: bool) -> Result<Option<NaiveDateTime>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if bound && s == "OPEN" => Ok(None),
        Some(Value::String(s)) => time::timestamp(s).map(Some),
        _ => Err(invalid(
            "Temporal fields must be strings or null in this profile",
        )),
    }
}
fn fact_interval(value: &Value) -> Result<time::Interval> {
    let fact = object(value)?;
    let start = date(fact.get("valid_from"), false)?;
    let end = date(fact.get("valid_until"), true)?;
    // The original wrapper parses these even though retroactive_coverage does
    // not use them. Retain parse failures. Its missing-recorded clock fallback
    // has no observable effect on this projection; no normalized fact is emitted.
    date(fact.get("recorded_at"), false)?;
    date(fact.get("superseded_at"), true)?;
    Ok(time::Interval {
        start: start.unwrap_or_else(|| {
            NaiveDate::from_ymd_opt(1, 1, 1)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap()
        }),
        end,
        label: None,
    })
}
pub(crate) fn list<'a>(graph: &'a Map<String, Value>, key: &str) -> Result<&'a [Value]> {
    match graph.get(key) {
        None => Ok(&[]),
        Some(Value::Array(values)) => Ok(values),
        _ => Err(invalid(
            "Graph collections must be arrays; missing means empty",
        )),
    }
}
fn timeline(request: &SemanticTemporalFactsRequest) -> Result<Vec<SemanticTemporalFactEvent>> {
    let graph = object(&request.graph)?;
    let entities = list(graph, "entities")?;
    let relationships = list(graph, "relationships")?;
    if entities.len().saturating_add(relationships.len()) > 4096 {
        return Err(invalid("More than4096 input records"));
    }
    let key = scalar_id(&request.entity_id)?;
    let missing = Value::Null;
    let empty = Value::String(String::new());
    let mut events = Vec::new();
    for (kind, records) in [("entities", entities), ("relationships", relationships)] {
        for (index, raw) in records.iter().enumerate() {
            let fact = object(raw)?;
            let selected = if kind == "entities" {
                scalar_id(
                    fact.get("id")
                        .unwrap_or_else(|| fact.get("name").unwrap_or(&empty)),
                )? == key
            } else {
                scalar_id(fact.get("source").unwrap_or(&missing))? == key
                    || scalar_id(fact.get("target").unwrap_or(&missing))? == key
            };
            if !selected {
                continue;
            }
            let start = date(fact.get("valid_from"), false)?;
            let end = date(fact.get("valid_until"), true)?;
            let recorded = date(fact.get("recorded_at"), false)?;
            let superseded = date(fact.get("superseded_at"), true)?;
            for (timestamp, change) in [
                (start, "added"),
                (recorded.filter(|v| Some(*v) != start), "modified"),
                (superseded, "modified"),
                (end, "removed"),
            ] {
                if let Some(timestamp) = timestamp {
                    events.push((
                        timestamp,
                        SemanticTemporalFactEvent {
                            timestamp: time::text(timestamp),
                            change_type: change.into(),
                            origin_kind: kind.into(),
                            origin_index: index as u64,
                            fact: raw.clone(),
                        },
                    ));
                }
            }
        }
    }
    // Compare datetimes, not variable-width timestamp text. Stable ties preserve
    // entity-before-relationship and original list/event order, including duplicates.
    events.sort_by(|a, b| (a.0, &a.1.change_type).cmp(&(b.0, &b.1.change_type)));
    Ok(events.into_iter().map(|(_, e)| e).collect())
}
impl SemanticTemporalFactsPort for RustSemanticRuntime {
    fn temporal_facts(
        &self,
        request: &SemanticTemporalFactsRequest,
    ) -> Result<SemanticTemporalFactsResult> {
        crate::datalog_port::check_serialized(request, MAX_INPUT_BYTES, false)
            .map_err(|_| invalid("Complete request exceeds1MiB"))?;
        if request.schema != "semantica.temporal-facts-request/1" || request.facts.len() > 4096 {
            return Err(invalid("Unknown schema or more than4096 originals"));
        }
        let shape = match request.operation.as_str() {
            "timeline" => {
                request.graph.is_object() && request.revision.is_null() && request.facts.is_empty()
            }
            "revision-impact" => {
                request.graph.is_null()
                    && request.entity_id.is_null()
                    && request.revision.is_object()
            }
            _ => false,
        };
        if !shape {
            return Err(invalid("Operation and supplied fields disagree"));
        }
        let mut result = SemanticTemporalFactsResult {
            schema: "semantica.temporal-facts-result/1".into(),
            profile: "semantica.source-temporal-facts/1".into(),
            request_sha256: crate::sha256(
                semantica_contracts::json::canonical_json(
                    &serde_json::to_value(request).map_err(invalid)?,
                )
                .map_err(invalid)?,
            ),
            operation: request.operation.clone(),
            events: vec![],
            affected: vec![],
            partial: vec![],
            unaffected: vec![],
        };
        if request.operation == "timeline" {
            result.events = timeline(request)?;
        } else {
            let revision = fact_interval(&request.revision)?;
            for (index, raw) in request.facts.iter().enumerate() {
                let original = fact_interval(raw)?;
                let reference = SemanticTemporalFactReference {
                    index: index as u64,
                    fact: raw.clone(),
                };
                if time::contains(&original, &revision)? {
                    result.affected.push(reference);
                } else if time::overlaps(time::relation(&original, &revision)?) {
                    result.partial.push(reference);
                } else {
                    result.unaffected.push(reference);
                }
            }
        }
        crate::datalog_port::check_serialized(&result, MAX_OUTPUT_BYTES - 1, true)
            .map_err(|_| invalid("Complete output exceeds16MiB including newline"))?;
        Ok(result)
    }
}
