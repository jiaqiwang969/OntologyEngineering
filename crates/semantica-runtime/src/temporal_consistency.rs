//! Bounded transport of the original TemporalGraphQuery consistency report.
//! Retains source defects and incomplete checks; a report is not engineering adoption.
use crate::{
    runtime::RustSemanticRuntime,
    temporal_facts::{date, list, object, scalar_id},
    temporal_intervals as time,
    temporal_snapshot::axis_bounds,
};
use chrono::{NaiveDate, NaiveDateTime};
use semantica_contracts::{
    SemanticRuntimeFailure, SemanticTemporalConsistencyIssue, SemanticTemporalConsistencyPort,
    SemanticTemporalConsistencyReport, SemanticTemporalConsistencyRequest,
    SemanticTemporalConsistencyResult,
};
use serde_json::{Map, Value, json};
use std::{cmp::Ordering, collections::HashMap};

pub const MAX_INPUT_BYTES: usize = 1024 * 1024;
pub const MAX_OUTPUT_BYTES: usize = 16 * 1024 * 1024;
const MAX_GROUP_COMPARISONS: u64 = 65536;
type Result<T> = std::result::Result<T, SemanticRuntimeFailure>;
type Bounds = (Option<NaiveDateTime>, Option<NaiveDateTime>);
fn invalid(message: impl ToString) -> SemanticRuntimeFailure {
    SemanticRuntimeFailure {
        code: "temporal_consistency_invalid".into(),
        phase: "execution".into(),
        message: message.to_string(),
        retryable: false,
    }
}
fn source_failure(kind: &str, message: &str) -> SemanticRuntimeFailure {
    SemanticRuntimeFailure {
        code: "temporal_consistency_source_failure".into(),
        phase: "execution".into(),
        message: format!("{kind}: {message}"),
        retryable: false,
    }
}
fn truthy(value: &Value) -> Result<bool> {
    Ok(match value {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(_) => !semantica_json::python_equal(value, &json!(0)).map_err(invalid)?,
        Value::String(v) => !v.is_empty(),
        Value::Array(v) => !v.is_empty(),
        Value::Object(v) => !v.is_empty(),
    })
}
fn relationship_id(relationship: &Map<String, Value>) -> Result<&Value> {
    let id = relationship.get("id").unwrap_or(&Value::Null);
    if !truthy(id)? {
        return Err(source_failure(
            "builtins.AttributeError",
            "'TemporalGraphQuery' object has no attribute '_relationship_key'",
        ));
    }
    Ok(id)
}
// A caught original TemporalValidationError is a report entry. Resource/domain
// refusals and other source exceptions abort the call, without a partial report.
fn parse_diagnostic(value: Option<&Value>, error: SemanticRuntimeFailure) -> Result<String> {
    match value {
        Some(Value::String(text)) if text.len() <= 256 => {
            if error.message == "Datetime arithmetic outside original year1..9999" {
                return Err(source_failure(
                    "builtins.OverflowError",
                    "date value out of range",
                ));
            }
            Ok(format!("Invalid temporal value | Context: value={text}"))
        }
        _ => Err(error),
    }
}
fn source_bounds(
    f: &Map<String, Value>,
    clock: NaiveDateTime,
    fallbacks: &mut u64,
) -> Result<std::result::Result<Bounds, String>> {
    match axis_bounds(f, "valid", clock, fallbacks) {
        Ok(bounds) => Ok(Ok(bounds)),
        Err(original) => {
            // Reuse the parser to identify the failed original field; this
            // diagnostic pass does not call the clock or normalize another fact.
            for (name, bound) in [
                ("valid_from", false),
                ("valid_until", true),
                ("recorded_at", false),
                ("superseded_at", true),
            ] {
                if let Err(error) = date(f.get(name), bound) {
                    return Ok(Err(parse_diagnostic(f.get(name), error)?));
                }
            }
            Err(original)
        }
    }
}
fn compare(a: NaiveDateTime, b: NaiveDateTime, granularity: &str) -> Result<Ordering> {
    let g = time::granularity(Some(granularity)).map_err(|_| {
        source_failure(
            "builtins.ValueError",
            &format!("Unsupported temporal granularity: {granularity}"),
        )
    })?;
    Ok(time::floor(a, g)?.cmp(&time::floor(b, g)?))
}
fn lifetime(
    entity: &Map<String, Value>,
    start: Option<NaiveDateTime>,
    end: Option<NaiveDateTime>,
    granularity: &str,
) -> Result<std::result::Result<bool, String>> {
    // _parse_time accepts OPEN as an unknown entity start; the fact parser does not.
    let entity_start = match date(entity.get("valid_from"), true) {
        Ok(v) => v,
        Err(e) => return Ok(Err(parse_diagnostic(entity.get("valid_from"), e)?)),
    };
    let entity_end = match date(entity.get("valid_until"), true) {
        Ok(v) => v,
        Err(e) => return Ok(Err(parse_diagnostic(entity.get("valid_until"), e)?)),
    };
    if let (Some(s), Some(es)) = (start, entity_start) {
        if compare(s, es, granularity)? == Ordering::Less {
            return Ok(Ok(false));
        }
    }
    if let Some(ee) = entity_end {
        if let Some(s) = start {
            if compare(s, ee, granularity)? == Ordering::Greater {
                return Ok(Ok(false));
            }
        }
        match end {
            Some(e) if compare(e, ee, granularity)? != Ordering::Greater => {}
            _ => return Ok(Ok(false)),
        }
    }
    Ok(Ok(true))
}
fn minimum() -> NaiveDateTime {
    NaiveDate::from_ymd_opt(1, 1, 1)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap()
}
fn maximum() -> NaiveDateTime {
    NaiveDate::from_ymd_opt(9999, 12, 31)
        .unwrap()
        .and_hms_micro_opt(23, 59, 59, 999999)
        .unwrap()
}
fn overlap(current: Bounds, previous: Bounds) -> Result<bool> {
    let query = time::Interval {
        start: current.0.unwrap_or_else(minimum),
        end: Some(current.1.unwrap_or_else(maximum)),
        label: None,
    };
    if query.end.is_some_and(|end| end < query.start)
        || previous.0.zip(previous.1).is_some_and(|(s, e)| e < s)
    {
        return Ok(false);
    }
    let candidate = time::Interval {
        start: previous.0.unwrap_or_else(minimum),
        end: previous.1,
        label: None,
    };
    Ok(!matches!(
        time::relation(&candidate, &query)?,
        "before" | "after"
    ))
}
fn push(
    issues: &mut Vec<SemanticTemporalConsistencyIssue>,
    positions: &mut Vec<u64>,
    index: usize,
    id: &Value,
    kind: &str,
    message: impl ToString,
) {
    issues.push(SemanticTemporalConsistencyIssue {
        message: message.to_string(),
        fact_id: id.clone(),
        issue_type: kind.into(),
    });
    positions.push(index as u64);
}
struct Group {
    key: [Value; 3],
    positions: Vec<usize>,
}
fn group_key(f: &Map<String, Value>) -> Result<[Value; 3]> {
    let empty = Value::String(String::new());
    let key = [
        f.get("source").unwrap_or(&empty),
        f.get("type")
            .unwrap_or_else(|| f.get("relationship").unwrap_or(&empty)),
        f.get("target").unwrap_or(&empty),
    ];
    for v in key {
        let kind = match v {
            Value::Array(_) => Some("list"),
            Value::Object(_) => Some("dict"),
            _ => None,
        };
        if let Some(kind) = kind {
            return Err(source_failure(
                "builtins.TypeError",
                &format!("unhashable type: '{kind}'"),
            ));
        }
    }
    Ok(key.map(Clone::clone))
}
impl SemanticTemporalConsistencyPort for RustSemanticRuntime {
    fn temporal_consistency(
        &self,
        r: &SemanticTemporalConsistencyRequest,
    ) -> Result<SemanticTemporalConsistencyResult> {
        crate::datalog_port::check_serialized(r, MAX_INPUT_BYTES, false)
            .map_err(|_| invalid("Complete request exceeds1MiB"))?;
        if r.schema != "semantica.temporal-consistency-request/1" {
            return Err(invalid("Unknown schema"));
        }
        let clock = time::timestamp(&r.clock_at)?;
        let graph = object(&r.graph)?;
        let entities = list(graph, "entities")?;
        let relationships = list(graph, "relationships")?;
        if entities.len().saturating_add(relationships.len()) > 4096 {
            return Err(invalid("More than4096 total input records"));
        }
        let mut entity_index = HashMap::new();
        let empty = Value::String(String::new());
        for raw in entities {
            let entity = object(raw)?;
            let key = scalar_id(
                entity
                    .get("id")
                    .unwrap_or_else(|| entity.get("name").unwrap_or(&empty)),
            )?;
            entity_index.insert(key, entity);
        }
        let mut out = SemanticTemporalConsistencyResult {
            schema: "semantica.temporal-consistency-result/1".into(),
            profile: "semantica.source-temporal-consistency/1".into(),
            request_sha256: crate::sha256(
                semantica_contracts::json::canonical_json(
                    &serde_json::to_value(r).map_err(invalid)?,
                )
                .map_err(invalid)?,
            ),
            clock_at: time::text(clock),
            report: SemanticTemporalConsistencyReport {
                errors: vec![],
                warnings: vec![],
            },
            error_relationship_indices: vec![],
            warning_relationship_indices: vec![],
            clock_fallback_count: 0,
            group_comparison_count: 0,
        };
        let mut groups: Vec<Group> = vec![];
        for (index, raw) in relationships.iter().enumerate() {
            let f = object(raw)?;
            let id = relationship_id(f)?;
            let key = group_key(f)?;
            let mut found = None;
            for (n, group) in groups.iter().enumerate() {
                if out.group_comparison_count == MAX_GROUP_COMPARISONS {
                    return Err(invalid("More than65536 group comparisons"));
                }
                out.group_comparison_count += 1;
                let mut equal = true;
                for (a, b) in group.key.iter().zip(&key) {
                    if !semantica_json::python_equal(a, b).map_err(invalid)? {
                        equal = false;
                        break;
                    }
                }
                if equal {
                    found = Some(n);
                    break;
                }
            }
            if let Some(n) = found {
                groups[n].positions.push(index);
            } else {
                groups.push(Group {
                    key,
                    positions: vec![index],
                });
            }
            let (start, end) = match source_bounds(f, clock, &mut out.clock_fallback_count)? {
                Ok(v) => v,
                Err(message) => {
                    push(
                        &mut out.report.errors,
                        &mut out.error_relationship_indices,
                        index,
                        id,
                        "invalid_temporal_fields",
                        format!("Unable to parse temporal fields: {message}"),
                    );
                    continue;
                }
            };
            if let (Some(s), Some(e)) = (start, end) {
                if compare(s, e, &r.granularity)? == Ordering::Greater {
                    push(
                        &mut out.report.errors,
                        &mut out.error_relationship_indices,
                        index,
                        id,
                        "inverted_interval",
                        "Relationship has an inverted validity interval.",
                    );
                }
            }
            for endpoint in ["source", "target"] {
                // Source index stringifies entity IDs but lookup uses the raw
                // endpoint. Numeric/null endpoints therefore remain missing.
                let entity = f
                    .get(endpoint)
                    .and_then(Value::as_str)
                    .and_then(|key| entity_index.get(key));
                let Some(entity) = entity else {
                    push(
                        &mut out.report.errors,
                        &mut out.error_relationship_indices,
                        index,
                        id,
                        &format!("missing_{endpoint}_entity"),
                        format!("Relationship references missing {endpoint} entity."),
                    );
                    continue;
                };
                match lifetime(entity, start, end, &r.granularity)? {
                    Err(message) => push(
                        &mut out.report.errors,
                        &mut out.error_relationship_indices,
                        index,
                        id,
                        &format!("invalid_{endpoint}_temporal_fields"),
                        format!("Unable to parse {endpoint} entity lifetime: {message}"),
                    ),
                    Ok(false) => push(
                        &mut out.report.errors,
                        &mut out.error_relationship_indices,
                        index,
                        id,
                        &format!("{endpoint}_lifetime_mismatch"),
                        format!("Relationship validity falls outside {endpoint} entity lifetime."),
                    ),
                    Ok(true) => {}
                }
            }
        }
        for group in groups {
            let mut safe = vec![];
            for index in group.positions {
                let f = object(&relationships[index])?;
                let id = relationship_id(f)?;
                match source_bounds(f, clock, &mut out.clock_fallback_count)? {
                    Ok(bounds) => safe.push((index, bounds)),
                    Err(message) => push(
                        &mut out.report.errors,
                        &mut out.error_relationship_indices,
                        index,
                        id,
                        "invalid_temporal_fields",
                        format!("Unable to parse temporal fields: {message}"),
                    ),
                }
            }
            safe.sort_by_key(|(_, bounds)| bounds.0.unwrap_or_else(minimum));
            for pair in safe.windows(2) {
                let (_, previous) = pair[0];
                let (index, current) = pair[1];
                let id = relationship_id(object(&relationships[index])?)?;
                let warning = if overlap(current, previous)? {
                    Some((
                        "overlapping_same_edge",
                        "Relationship overlaps another relationship with the same edge and type.",
                    ))
                } else if let (Some(pe), Some(cs)) = (previous.1, current.0) {
                    if compare(cs, pe, &r.granularity)? == Ordering::Greater {
                        Some((
                            "gap_after_restart",
                            "Relationship restarts after a temporal gap.",
                        ))
                    } else if let Some(ps) = previous.0 {
                        if compare(cs, ps, &r.granularity)? == Ordering::Equal {
                            Some((
                                "overlapping_same_edge",
                                "Relationship interval overlaps another interval on the same edge.",
                            ))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else if let (Some(cs), Some(ps)) = (current.0, previous.0) {
                    if compare(cs, ps, &r.granularity)? == Ordering::Equal {
                        Some((
                            "overlapping_same_edge",
                            "Relationship interval overlaps another interval on the same edge.",
                        ))
                    } else {
                        None
                    }
                } else {
                    None
                };
                if let Some((kind, message)) = warning {
                    push(
                        &mut out.report.warnings,
                        &mut out.warning_relationship_indices,
                        index,
                        id,
                        kind,
                        message,
                    );
                }
            }
        }
        crate::datalog_port::check_serialized(&out, MAX_OUTPUT_BYTES, true)
            .map_err(|_| invalid("Complete pretty result exceeds16MiB"))?;
        Ok(out)
    }
}
