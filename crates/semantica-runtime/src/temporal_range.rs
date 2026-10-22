//! Bounded source TemporalGraphQuery range filtering and calendar grouping.
//! Source kg/temporal_query.py (MIT); inclusive range contact differs from the
//! half-open point owner. No engineering applicability follows from this view.
use crate::{
    runtime::RustSemanticRuntime,
    temporal_facts::{list, object},
    temporal_intervals as time,
    temporal_snapshot::axis_bounds,
};
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime};
use semantica_contracts::{
    SemanticRuntimeFailure, SemanticTemporalRangePort, SemanticTemporalRangeQueryView,
    SemanticTemporalRangeRequest, SemanticTemporalRangeResult,
};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

pub const MAX_INPUT_BYTES: usize = 1024 * 1024;
pub const MAX_OUTPUT_BYTES: usize = 16 * 1024 * 1024;
const MAX_PERIODS: usize = 4096;
const MAX_PERIOD_CHECKS: usize = 65536;
type Result<T> = std::result::Result<T, SemanticRuntimeFailure>;
fn invalid(message: impl ToString) -> SemanticRuntimeFailure {
    SemanticRuntimeFailure {
        code: "temporal_range_invalid".into(),
        phase: "execution".into(),
        message: message.to_string(),
        retryable: false,
    }
}
fn minimum() -> NaiveDateTime {
    NaiveDate::from_ymd_opt(1, 1, 1)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap()
}
fn matches_range(
    f: &Map<String, Value>,
    axis: &str,
    domain: &time::Interval,
    clock: NaiveDateTime,
    fallbacks: &mut u64,
    covers: bool,
) -> Result<bool> {
    let axes: &[&str] = if axis == "both" {
        &["valid", "transaction"]
    } else {
        &[axis]
    };
    for axis in axes {
        // Source normalizes all four fields before it checks the axis or bounds.
        let (start, end) = axis_bounds(f, axis, clock, fallbacks)?;
        if domain.end.is_some_and(|v| v < domain.start)
            || start.zip(end).is_some_and(|(s, e)| e < s)
        {
            return Ok(false);
        }
        let candidate = time::Interval {
            start: start.unwrap_or_else(minimum),
            end,
            label: None,
        };
        let matched = if covers {
            time::contains(&candidate, domain)?
        } else {
            !matches!(time::relation(&candidate, domain)?, "before" | "after")
        };
        if !matched {
            return Ok(false);
        }
    }
    Ok(true)
}
fn next_period(v: NaiveDateTime, g: &str) -> Result<NaiveDateTime> {
    // These are the original graph-query steps, not interval end expansion:
    // second/minute/hour deliberately use the source fallback of one day.
    let next = match g {
        "month" if v.month() == 12 => v.with_year(v.year() + 1).and_then(|v| v.with_month(1)),
        "month" => v.with_month(v.month() + 1),
        "year" => v.with_year(v.year() + 1),
        "week" => v.checked_add_signed(Duration::weeks(1)),
        _ => v.checked_add_signed(Duration::days(1)),
    };
    next.filter(|v| (1..=9999).contains(&v.year()))
        .ok_or_else(|| invalid("Calendar step outside original year1..9999"))
}
fn period_label(v: NaiveDateTime, g: &str) -> String {
    match g {
        "week" => format!("{}-W{:02}", v.iso_week().year(), v.iso_week().week()),
        "month" => v.format("%Y-%m").to_string(),
        "year" => v.format("%Y").to_string(),
        _ => v.format("%Y-%m-%d").to_string(),
    }
}
impl SemanticTemporalRangePort for RustSemanticRuntime {
    fn temporal_range(
        &self,
        r: &SemanticTemporalRangeRequest,
    ) -> Result<SemanticTemporalRangeResult> {
        crate::datalog_port::check_serialized(r, MAX_INPUT_BYTES, false)
            .map_err(|_| invalid("Complete request exceeds1MiB"))?;
        if r.schema != "semantica.temporal-range-request/1" {
            return Err(invalid("Unknown schema"));
        }
        let clock = time::timestamp(&r.clock_at)?;
        let graph = object(&r.graph)?;
        let g = time::granularity(Some(&r.granularity))?;
        let start = time::floor(
            time::timestamp(
                r.start_time
                    .as_deref()
                    .ok_or_else(|| invalid("Range start must be a concrete timestamp"))?,
            )?,
            g,
        )?;
        let end = match r.end_time.as_deref() {
            None | Some("OPEN") => None,
            Some(v) => Some(time::expand(time::timestamp(v)?, g)?),
        };
        let domain = time::Interval {
            start,
            end,
            label: None,
        };
        let relationships = list(graph, "relationships")?;
        if relationships.len() > 4096 {
            return Err(invalid("More than4096 relationships"));
        }
        let mut indices = vec![];
        let mut fallbacks = 0;
        for (i, raw) in relationships.iter().enumerate() {
            if matches_range(
                object(raw)?,
                &r.time_axis,
                &domain,
                clock,
                &mut fallbacks,
                false,
            )? {
                indices.push(i as u64);
            }
        }
        if r.aggregation == "intersection" {
            let mut complete = vec![];
            for &i in &indices {
                if matches_range(
                    object(&relationships[i as usize])?,
                    &r.time_axis,
                    &domain,
                    clock,
                    &mut fallbacks,
                    true,
                )? {
                    complete.push(i);
                }
            }
            indices = complete;
        }
        let mut buckets: Option<BTreeMap<String, Vec<u64>>> = None;
        if r.aggregation == "evolution" {
            let mut groups = BTreeMap::<String, Vec<u64>>::new();
            let mut current = time::floor(start, g)?;
            let end_bound = time::floor(
                end.ok_or_else(|| invalid("Calendar evolution needs a finite end"))?,
                g,
            )?;
            let mut periods = 0usize;
            let mut checks = 0usize;
            while current <= end_bound {
                periods += 1;
                checks = checks.saturating_add(indices.len());
                if periods > MAX_PERIODS || checks > MAX_PERIOD_CHECKS {
                    return Err(invalid(
                        "Calendar period or relationship-check budget exceeded",
                    ));
                }
                let next = next_period(current, g)?;
                let period = time::Interval {
                    start: current,
                    end: Some(next),
                    label: None,
                };
                let label = period_label(current, g);
                for &i in &indices {
                    if matches_range(
                        object(&relationships[i as usize])?,
                        "valid",
                        &period,
                        clock,
                        &mut fallbacks,
                        false,
                    )? {
                        groups.entry(label.clone()).or_default().push(i);
                    }
                }
                current = next;
            }
            buckets = Some(groups);
        }
        // Repeated large records can exceed the output budget despite a small
        // input. Count compact payload bytes before cloning; pretty full output
        // is at least this large and receives a second, exact check below.
        let sizes = relationships
            .iter()
            .map(|v| serde_json::to_vec(v).map(|v| v.len()).map_err(invalid))
            .collect::<Result<Vec<_>>>()?;
        let mut bytes = 0usize;
        for &i in indices
            .iter()
            .chain(buckets.iter().flat_map(|b| b.values().flatten()))
        {
            bytes = bytes.saturating_add(sizes[i as usize]);
            if bytes > MAX_OUTPUT_BYTES {
                return Err(invalid("Repeated relationship payload exceeds16MiB"));
            }
        }
        let values = |ids: &[u64]| {
            ids.iter()
                .map(|&i| relationships[i as usize].clone())
                .collect::<Vec<_>>()
        };
        let grouped = match &buckets {
            None => Value::Null,
            Some(b) => Value::Object(
                b.iter()
                    .map(|(k, v)| (k.clone(), Value::Array(values(v))))
                    .collect(),
            ),
        };
        let result = SemanticTemporalRangeResult {
            schema: "semantica.temporal-range-result/1".into(),
            profile: "semantica.source-temporal-range/1".into(),
            request_sha256: crate::sha256(
                semantica_contracts::json::canonical_json(
                    &serde_json::to_value(r).map_err(invalid)?,
                )
                .map_err(invalid)?,
            ),
            clock_at: time::text(clock),
            query_result: SemanticTemporalRangeQueryView {
                query: r.query.clone(),
                start_time: time::text(start),
                end_time: end.map(time::text),
                relationships: values(&indices),
                relationship_buckets: grouped,
                num_relationships: indices.len() as u64,
                aggregation: r.aggregation.clone(),
            },
            relationship_indices: indices,
            bucket_indices: serde_json::to_value(buckets).map_err(invalid)?,
            clock_fallback_count: fallbacks,
        };
        crate::datalog_port::check_serialized(&result, MAX_OUTPUT_BYTES, true)
            .map_err(|_| invalid("Complete pretty result exceeds16MiB including newline"))?;
        Ok(result)
    }
}
