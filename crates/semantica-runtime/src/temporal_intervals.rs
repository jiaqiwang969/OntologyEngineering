//! Bounded string transport of the locked Semantica temporal interval engine.
//! Source: semantica/kg/temporal_reasoning.py and temporal_model.py (MIT).
//! Date parsing delegates to the existing CPython-compatible temporal owner.
//! Duration coverage is not engineering evidence quality or causal confidence.
use crate::runtime::RustSemanticRuntime;
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Timelike};
use semantica_contracts::{
    SemanticRuntimeFailure, SemanticTemporalInterval, SemanticTemporalPort,
    SemanticTemporalRequest, SemanticTemporalResult,
};
use std::sync::OnceLock;

pub const MAX_INPUT_BYTES: usize = 1024 * 1024;
pub const MAX_OUTPUT_BYTES: usize = 16 * 1024 * 1024;
pub const PROFILE: &str = "semantica.source-temporal-intervals/1";
type Result<T> = std::result::Result<T, SemanticRuntimeFailure>;

fn fail(code: &str, phase: &str, message: impl ToString) -> SemanticRuntimeFailure {
    SemanticRuntimeFailure {
        code: code.into(),
        phase: phase.into(),
        message: message.to_string(),
        retryable: false,
    }
}
fn invalid(message: impl ToString) -> SemanticRuntimeFailure {
    fail("temporal_value_invalid", "execution", message)
}
fn bounded(value: Option<NaiveDateTime>) -> Result<NaiveDateTime> {
    value
        .filter(|v| (1..=9999).contains(&v.year()))
        .ok_or_else(|| invalid("Datetime arithmetic outside original year1..9999"))
}
pub(crate) fn timestamp(raw: &str) -> Result<NaiveDateTime> {
    if raw.len() > 256 {
        return Err(fail(
            "temporal_timestamp_budget",
            "contract",
            "Timestamp exceeds256 UTF8 bytes",
        ));
    }
    let text = raw.trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c));
    static PAD: OnceLock<regex::Regex> = OnceLock::new();
    let re = PAD.get_or_init(|| {
        regex::Regex::new(r"^([0-9]{4})-([0-9]{1,2})-([0-9]{1,2})(.*)$").expect("fixed pattern")
    });
    let padded;
    let text = if let Some(m) = re.captures(text) {
        padded = format!(
            "{}-{:02}-{:02}{}",
            &m[1],
            m[2].parse::<u32>().expect("digits"),
            m[3].parse::<u32>().expect("digits"),
            &m[4]
        );
        &padded
    } else {
        text
    };
    let parsed = semantica_temporal::parse_datetime(text)
        .ok_or_else(|| invalid("Unsupported or invalid temporal string"))?;
    let (y, m, d) = parsed.date_parts();
    let [h, minute, s, us] = parsed.time_parts();
    let local = NaiveDate::from_ymd_opt(y, m, d)
        .and_then(|v| v.and_hms_micro_opt(h, minute, s, us))
        .ok_or_else(|| invalid("Invalid datetime components"))?;
    // The source knowledge-graph parser assumes UTC for naive values. This does
    // not change the distinct naive/aware RDF literal comparison contract.
    bounded(local.checked_sub_signed(Duration::microseconds(parsed.offset_micros().unwrap_or(0))))
}
pub(crate) fn text(value: NaiveDateTime) -> String {
    let mut s = value.format("%Y-%m-%dT%H:%M:%S").to_string();
    if value.nanosecond() != 0 {
        s.push_str(&format!(".{:06}", value.nanosecond() / 1000));
    }
    s.push('Z');
    s
}
fn maximum() -> NaiveDateTime {
    NaiveDate::from_ymd_opt(9999, 12, 31)
        .unwrap()
        .and_hms_micro_opt(23, 59, 59, 999999)
        .unwrap()
}
#[derive(Clone)]
pub(crate) struct Interval {
    pub(crate) start: NaiveDateTime,
    pub(crate) end: Option<NaiveDateTime>,
    pub(crate) label: Option<String>,
}
impl Interval {
    fn parse(value: &SemanticTemporalInterval) -> Result<Self> {
        if value.label.as_ref().is_some_and(|v| v.len() > 4096) {
            return Err(fail(
                "temporal_label_budget",
                "contract",
                "Label exceeds4096 UTF8 bytes",
            ));
        }
        Ok(Self {
            start: timestamp(&value.start)?,
            end: match value.end.as_deref() {
                None | Some("OPEN") => None,
                Some(v) => Some(timestamp(v)?),
            },
            label: value.label.clone(),
        })
    }
    fn validate(&self) -> Result<()> {
        if self.end.is_some_and(|end| self.start > end) {
            return Err(invalid("Temporal intervals must satisfy start <= end."));
        }
        Ok(())
    }
    fn end_value(&self) -> NaiveDateTime {
        self.end.unwrap_or_else(maximum)
    }
    fn wire(&self) -> SemanticTemporalInterval {
        SemanticTemporalInterval {
            start: text(self.start),
            end: self.end.map(text),
            label: self.label.clone(),
        }
    }
}
pub(crate) fn active_at(
    mut interval: Interval,
    mut point: NaiveDateTime,
    precision: Option<&str>,
) -> Result<bool> {
    interval.validate()?;
    if let Some(g) = precision {
        let g = granularity(Some(g))?;
        point = floor(point, g)?;
        interval.start = floor(interval.start, g)?;
        interval.end = interval.end.map(|v| floor(v, g)).transpose()?;
    }
    Ok(interval.start <= point && interval.end.is_none_or(|end| point < end))
}
pub(crate) fn contains(a: &Interval, b: &Interval) -> Result<bool> {
    a.validate()?;
    b.validate()?;
    Ok(a.start <= b.start && a.end_value() >= b.end_value())
}
pub(crate) fn relation(a: &Interval, b: &Interval) -> Result<&'static str> {
    a.validate()?;
    b.validate()?;
    let ae = a.end_value();
    let be = b.end_value();
    // Order is source-significant for zero-length intervals: equal points meet.
    Ok(if ae < b.start {
        "before"
    } else if a.start > be {
        "after"
    } else if ae == b.start {
        "meets"
    } else if a.start == be {
        "met_by"
    } else if a.start == b.start && ae == be {
        "equals"
    } else if a.start == b.start && ae < be {
        "starts"
    } else if a.start == b.start && ae > be {
        "started_by"
    } else if ae == be && a.start > b.start {
        "finishes"
    } else if ae == be && a.start < b.start {
        "finished_by"
    } else if a.start < b.start && ae > b.start && ae < be {
        "overlaps"
    } else if a.start > b.start && a.start < be && ae > be {
        "overlapped_by"
    } else if a.start > b.start && ae < be {
        "during"
    } else {
        "contains"
    })
}
pub(crate) fn overlaps(relation: &str) -> bool {
    !matches!(relation, "before" | "after" | "meets" | "met_by")
}
fn merge(mut intervals: Vec<Interval>) -> Result<Vec<Interval>> {
    for interval in &intervals {
        interval.validate()?;
    }
    intervals.sort_by_key(|i| i.start); // Stable: first label survives equal starts.
    let mut result: Vec<Interval> = Vec::with_capacity(intervals.len());
    for interval in intervals {
        if let Some(last) = result
            .last_mut()
            .filter(|v| interval.start <= v.end_value())
        {
            last.end = match (last.end, interval.end) {
                (Some(a), Some(b)) => Some(a.max(b)),
                _ => None,
            };
        } else {
            result.push(interval);
        }
    }
    Ok(result)
}
fn clipped(intervals: Vec<Interval>, domain: &Interval) -> Result<Vec<Interval>> {
    let end = domain
        .end
        .ok_or_else(|| invalid("Coverage and gaps require a finite domain end"))?;
    let mut result = Vec::with_capacity(intervals.len());
    for interval in intervals {
        interval.validate()?;
        if !overlaps(relation(&interval, domain)?)
            && interval.end != Some(domain.start)
            && interval.start != end
        {
            continue;
        }
        let start = interval.start.max(domain.start);
        let stop = interval.end.unwrap_or(end).min(end);
        if start < stop {
            result.push(Interval {
                start,
                end: Some(stop),
                label: interval.label,
            });
        }
    }
    merge(result)
}
pub(crate) fn granularity(value: Option<&str>) -> Result<&str> {
    match value {
        Some(v @ ("second" | "minute" | "hour" | "day" | "week" | "month" | "year")) => Ok(v),
        _ => Err(invalid("Unsupported or missing temporal granularity")),
    }
}
pub(crate) fn floor(value: NaiveDateTime, g: &str) -> Result<NaiveDateTime> {
    let date = value.date();
    let (date, h, m, s) = match g {
        "second" => (date, value.hour(), value.minute(), value.second()),
        "minute" => (date, value.hour(), value.minute(), 0),
        "hour" => (date, value.hour(), 0, 0),
        "day" => (date, 0, 0, 0),
        "week" => (
            date.checked_sub_signed(Duration::days(i64::from(
                date.weekday().num_days_from_monday(),
            )))
            .ok_or_else(|| invalid("Week floor overflow"))?,
            0,
            0,
            0,
        ),
        "month" => (
            NaiveDate::from_ymd_opt(value.year(), value.month(), 1).unwrap(),
            0,
            0,
            0,
        ),
        "year" => (
            NaiveDate::from_ymd_opt(value.year(), 1, 1).unwrap(),
            0,
            0,
            0,
        ),
        _ => return Err(invalid("Unsupported temporal granularity")),
    };
    bounded(date.and_hms_micro_opt(h, m, s, 0))
}
pub(crate) fn expand(value: NaiveDateTime, g: &str) -> Result<NaiveDateTime> {
    let f = floor(value, g)?;
    let seconds = match g {
        "second" => 1,
        "minute" => 60,
        "hour" => 3600,
        "day" => 86400,
        "week" => 604800,
        _ => 0,
    };
    if seconds != 0 {
        // Original Python overflows on adding the next boundary before it
        // subtracts1microsecond. Do not silently clamp at datetime.max.
        let next = bounded(f.checked_add_signed(Duration::seconds(seconds)))?;
        return bounded(next.checked_sub_signed(Duration::microseconds(1)));
    }
    let month = if g == "year" { 12 } else { f.month() };
    let day = (28..=31)
        .rev()
        .find(|d| NaiveDate::from_ymd_opt(f.year(), month, *d).is_some())
        .unwrap();
    bounded(
        NaiveDate::from_ymd_opt(f.year(), month, day)
            .and_then(|d| d.and_hms_micro_opt(23, 59, 59, 999999)),
    )
}
fn seconds(start: NaiveDateTime, end: NaiveDateTime) -> f64 {
    (end - start)
        .num_microseconds()
        .expect("year1..9999 microsecond difference fits i64") as f64
        / 1_000_000.0
}

/// One immutable local request: no time-of-day lookup, graph, API or Project write.
impl SemanticTemporalPort for RustSemanticRuntime {
    fn temporal(&self, request: &SemanticTemporalRequest) -> Result<SemanticTemporalResult> {
        // Reuse only the existing bounded serde byte sink, not Datalog semantics.
        crate::datalog_port::check_serialized(request, MAX_INPUT_BYTES, false).map_err(|_| {
            fail(
                "temporal_input_byte_budget",
                "contract",
                "Complete temporal request exceeds1MiB",
            )
        })?;
        if request.schema != "semantica.temporal-request/1" || request.intervals.len() > 4096 {
            return Err(fail(
                "temporal_request_invalid",
                "contract",
                "Unknown schema or more than4096 intervals",
            ));
        }
        let n = request.intervals.len();
        let p = request.point.is_some();
        let d = request.domain.is_some();
        let g = request.granularity.is_some();
        let shape = match request.operation.as_str() {
            "compare" => n == 2 && !p && !d && !g,
            "active" => n == 1 && p && !d,
            "merge" => !p && !d && !g,
            "gaps" | "coverage" => !p && d && !g,
            "normalize-point" => n == 0 && p && !d && g,
            "normalize-interval" => n == 1 && !p && !d && g,
            _ => false,
        };
        if !shape {
            return Err(fail(
                "temporal_operation_shape",
                "contract",
                "Operation and supplied fields disagree",
            ));
        }
        let mut intervals = request
            .intervals
            .iter()
            .map(Interval::parse)
            .collect::<Result<Vec<_>>>()?;
        let mut result = SemanticTemporalResult {
            schema: "semantica.temporal-result/1".into(),
            profile: PROFILE.into(),
            request_sha256: crate::sha256(
                semantica_contracts::json::canonical_json(
                    &serde_json::to_value(request).map_err(|e| invalid(e))?,
                )
                .map_err(|e| invalid(e))?,
            ),
            operation: request.operation.clone(),
            relation: None,
            overlaps: None,
            contains: None,
            active: None,
            intervals: Vec::new(),
            point: None,
            coverage_fraction: None,
        };
        match request.operation.as_str() {
            "compare" => {
                let a = &intervals[0];
                let b = &intervals[1];
                let rel = relation(a, b)?;
                result.relation = Some(rel.into());
                result.overlaps = Some(overlaps(rel));
                result.contains = Some(contains(a, b)?);
            }
            "active" => {
                let interval = intervals.remove(0);
                interval.validate()?;
                let point = timestamp(request.point.as_deref().unwrap())?;
                result.active = Some(active_at(interval, point, request.granularity.as_deref())?);
            }
            "merge" => result.intervals = merge(intervals)?.iter().map(Interval::wire).collect(),
            "gaps" | "coverage" => {
                let domain = Interval::parse(request.domain.as_ref().unwrap())?;
                domain.validate()?;
                let end = domain
                    .end
                    .ok_or_else(|| invalid("Finite domain required"))?;
                let duration = seconds(domain.start, end);
                if request.operation == "coverage" && duration <= 0.0 {
                    result.coverage_fraction = Some(0.0);
                } else {
                    let merged = clipped(intervals, &domain)?;
                    if request.operation == "coverage" {
                        let covered: f64 = merged
                            .iter()
                            .map(|v| seconds(v.start, v.end.unwrap_or(end)))
                            .fold(0.0, |covered, next| covered + next);
                        let ratio = covered / duration;
                        // Source max(0.0, min(1.0, ratio)) keeps the left +0.0
                        // on equal zero; Rust sum/clamp can preserve -0.0.
                        result.coverage_fraction = Some(if ratio <= 0.0 {
                            0.0
                        } else if ratio >= 1.0 {
                            1.0
                        } else {
                            ratio
                        });
                    } else {
                        let mut cursor = domain.start;
                        for interval in merged {
                            if cursor < interval.start {
                                result.intervals.push(
                                    Interval {
                                        start: cursor,
                                        end: Some(interval.start),
                                        label: Some("gap".into()),
                                    }
                                    .wire(),
                                );
                            }
                            cursor = cursor.max(interval.end.unwrap_or(end));
                        }
                        if cursor < end {
                            result.intervals.push(
                                Interval {
                                    start: cursor,
                                    end: Some(end),
                                    label: Some("gap".into()),
                                }
                                .wire(),
                            );
                        }
                    }
                }
            }
            "normalize-point" => {
                let g = granularity(request.granularity.as_deref())?;
                result.point = Some(text(floor(
                    timestamp(request.point.as_deref().unwrap())?,
                    g,
                )?));
            }
            "normalize-interval" => {
                let g = granularity(request.granularity.as_deref())?;
                let interval = intervals.remove(0);
                // Source normalize_interval does not validate final ordering and
                // does not preserve a supplied label. Preserve both behaviors.
                result.intervals.push(
                    Interval {
                        start: floor(interval.start, g)?,
                        end: interval.end.map(|v| expand(v, g)).transpose()?,
                        label: None,
                    }
                    .wire(),
                );
            }
            _ => unreachable!("shape checked"),
        }
        crate::datalog_port::check_serialized(&result, MAX_OUTPUT_BYTES - 1, true).map_err(
            |_| {
                fail(
                    "temporal_output_byte_budget",
                    "result",
                    "Complete temporal result exceeds16MiB including newline",
                )
            },
        )?;
        Ok(result)
    }
}
