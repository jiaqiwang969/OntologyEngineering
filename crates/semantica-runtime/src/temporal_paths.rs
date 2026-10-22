//! Source temporal BFS. Timestamp ordering is a model condition, not causality.
use crate::{
    runtime::RustSemanticRuntime,
    temporal_facts::{date, list, object},
    temporal_intervals as time,
    temporal_snapshot::axis_bounds,
};
use chrono::NaiveDateTime;
use semantica_contracts::{
    SemanticRuntimeFailure, SemanticTemporalPath, SemanticTemporalPathsPort,
    SemanticTemporalPathsReport, SemanticTemporalPathsRequest, SemanticTemporalPathsResult,
};
use serde::Serialize;
use serde_json::{Map, Value};
use std::{cmp::Ordering, collections::VecDeque};

pub const MAX_INPUT_BYTES: usize = 1024 * 1024;
pub const MAX_OUTPUT_BYTES: usize = 16 * 1024 * 1024;
const MAX_IDENTITIES: u64 = 16_777_216;
const MAX_PATH_CELLS: usize = 1_048_576;
type Result<T> = std::result::Result<T, SemanticRuntimeFailure>;
fn invalid(message: impl ToString) -> SemanticRuntimeFailure {
    SemanticRuntimeFailure {
        code: "temporal_paths_invalid".into(),
        phase: "execution".into(),
        message: message.to_string(),
        retryable: false,
    }
}
fn source_failure(kind: &str, message: &str) -> SemanticRuntimeFailure {
    SemanticRuntimeFailure {
        code: "temporal_paths_source_failure".into(),
        phase: "execution".into(),
        message: format!("{kind}: {message}"),
        retryable: false,
    }
}
fn parse_failure(
    value: Option<&Value>,
    error: SemanticRuntimeFailure,
    window: bool,
) -> SemanticRuntimeFailure {
    match value {
        Some(Value::String(text)) if text.len() <= 256 => {
            if !window && error.message == "Datetime arithmetic outside original year1..9999" {
                source_failure("builtins.OverflowError", "date value out of range")
            } else {
                source_failure(
                    "semantica.utils.exceptions.TemporalValidationError",
                    &format!("Invalid temporal value | Context: value={text}"),
                )
            }
        }
        _ => error,
    }
}
fn window_date(value: Option<&Value>) -> Result<Option<NaiveDateTime>> {
    date(value, true).map_err(|e| parse_failure(value, e, true))
}
pub(crate) fn bounds(
    f: &Map<String, Value>,
    clock: NaiveDateTime,
    fallbacks: &mut u64,
) -> Result<(Option<NaiveDateTime>, Option<NaiveDateTime>)> {
    axis_bounds(f, "valid", clock, fallbacks).map_err(|original| {
        for (name, bound) in [
            ("valid_from", false),
            ("valid_until", true),
            ("recorded_at", false),
            ("superseded_at", true),
        ] {
            if let Err(error) = date(f.get(name), bound) {
                return parse_failure(f.get(name), error, false);
            }
        }
        original
    })
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
fn hashable(value: &Value) -> Result<()> {
    let kind = match value {
        Value::Array(_) => "list",
        Value::Object(_) => "dict",
        _ => return Ok(()),
    };
    Err(source_failure(
        "builtins.TypeError",
        &format!("unhashable type: '{kind}'"),
    ))
}
#[derive(Default)]
struct Comparisons(u64);
impl Comparisons {
    fn equal(&mut self, a: &Value, b: &Value) -> Result<bool> {
        if self.0 == MAX_IDENTITIES {
            return Err(invalid("More than16777216 identity comparisons"));
        }
        self.0 += 1;
        semantica_json::python_equal(a, b).map_err(invalid)
    }
    fn contains(&mut self, values: &[&Value], needle: &Value) -> Result<bool> {
        for value in values {
            if self.equal(value, needle)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
fn endpoint<'a>(f: &'a Map<String, Value>, key: &str, canonical: &str) -> &'a Value {
    match f.get(key) {
        None | Some(Value::Null) => f.get(canonical).unwrap_or(&Value::Null),
        Some(v) => v,
    }
}
struct Adjacency<'a> {
    source: &'a Value,
    neighbors: Vec<(&'a Value, usize)>,
}
#[derive(Serialize)]
struct PathRefs<'a> {
    path: Vec<&'a Value>,
    edges: Vec<&'a Value>,
    length: u64,
}
#[derive(Serialize)]
struct ReportRefs<'a> {
    source: &'a Value,
    target: &'a Value,
    paths: Vec<PathRefs<'a>>,
    num_paths: u64,
}
struct Pending<'a> {
    nodes: Vec<&'a Value>,
    edges: Vec<usize>,
}
fn permitted(
    r: &SemanticTemporalPathsRequest,
    rels: &[Value],
    previous: Option<usize>,
    current: usize,
    clock: NaiveDateTime,
    fallbacks: &mut u64,
) -> Result<bool> {
    if !r.enforce_causal_ordering || r.ordering_strategy == "loose" || previous.is_none() {
        return Ok(true);
    }
    let (previous_start, previous_end) =
        bounds(object(&rels[previous.unwrap()])?, clock, fallbacks)?;
    let (current_start, _) = bounds(object(&rels[current])?, clock, fallbacks)?;
    match r.ordering_strategy.as_str() {
        "strict" => match (previous_start, current_start) {
            (Some(p), Some(c)) => Ok(compare(c, p, &r.granularity)? != Ordering::Less),
            _ => Ok(true),
        },
        "overlap" => match (previous_end, current_start) {
            (Some(p), Some(c)) => Ok(compare(c, p, &r.granularity)? != Ordering::Greater),
            _ => Ok(true),
        },
        _ => Ok(true),
    }
}
impl SemanticTemporalPathsPort for RustSemanticRuntime {
    fn temporal_paths(
        &self,
        r: &SemanticTemporalPathsRequest,
    ) -> Result<SemanticTemporalPathsResult> {
        crate::datalog_port::check_serialized(r, MAX_INPUT_BYTES, false)?;
        if r.schema != "semantica.temporal-paths-request/1" {
            return Err(invalid("Unknown temporal paths request schema"));
        }
        let clock = time::timestamp(&r.clock_at)?;
        let graph = object(&r.graph)?;
        let rels = list(graph, "relationships")?;
        if rels.len() > 4096 {
            return Err(invalid("More than4096 relationships"));
        }
        let start = r.start_time.as_deref().filter(|s| !s.is_empty());
        let end = r.end_time.as_deref().filter(|s| !s.is_empty());
        let parsed_start = start
            .map(|s| window_date(Some(&Value::String(s.into()))))
            .transpose()?
            .flatten();
        let parsed_end = end
            .map(|s| window_date(Some(&Value::String(s.into()))))
            .transpose()?
            .flatten();
        let mut identities = Comparisons::default();
        let mut adjacency: Vec<Adjacency<'_>> = vec![];
        for (index, raw) in rels.iter().enumerate() {
            let f = object(raw)?;
            let s = endpoint(f, "source", "source_id");
            let t = endpoint(f, "target", "target_id");
            if start.is_some() || end.is_some() {
                let valid_from = window_date(f.get("valid_from"))?;
                let valid_until = window_date(f.get("valid_until"))?;
                if let (Some(p), Some(v)) = (parsed_start, valid_until) {
                    if compare(v, p, &r.granularity)? == Ordering::Less {
                        continue;
                    }
                }
                if let (Some(p), Some(v)) = (parsed_end, valid_from) {
                    if compare(v, p, &r.granularity)? == Ordering::Greater {
                        continue;
                    }
                }
            }
            hashable(s)?;
            let mut found = None;
            for (i, group) in adjacency.iter().enumerate() {
                if identities.equal(group.source, s)? {
                    found = Some(i);
                    break;
                }
            }
            match found {
                Some(i) => adjacency[i].neighbors.push((t, index)),
                None => adjacency.push(Adjacency {
                    source: s,
                    neighbors: vec![(t, index)],
                }),
            }
        }
        let mut queue = VecDeque::from([Pending {
            nodes: vec![&r.source],
            edges: vec![],
        }]);
        let mut visited = vec![];
        let mut accepted = vec![];
        let (mut fallbacks, mut expansions, mut queued) = (0, 0_u64, 1_u64);
        let mut cells = 1_usize;
        while let Some(path) = queue.pop_front() {
            // Original checks vertex count BEFORE target and before visited.
            if r.max_path_length
                .is_some_and(|max| max != 0 && path.nodes.len() as i64 > max)
            {
                continue;
            }
            let node = *path.nodes.last().unwrap();
            if identities.equal(node, &r.target)? {
                accepted.push(path);
                continue;
            }
            hashable(node)?;
            if identities.contains(&visited, node)? {
                continue;
            }
            visited.push(node);
            let mut found = None;
            for group in &adjacency {
                if identities.equal(group.source, node)? {
                    found = Some(group);
                    break;
                }
            }
            if let Some(group) = found {
                for &(neighbor, index) in &group.neighbors {
                    if expansions == 65536 {
                        return Err(invalid("More than65536 edge expansions"));
                    }
                    expansions += 1;
                    if identities.contains(&path.nodes, neighbor)? {
                        continue;
                    }
                    if permitted(
                        r,
                        rels,
                        path.edges.last().copied(),
                        index,
                        clock,
                        &mut fallbacks,
                    )? {
                        let extra = path.nodes.len() + path.edges.len() + 2;
                        if extra > MAX_PATH_CELLS - cells {
                            return Err(invalid("More than1048576 cumulative queued path cells"));
                        }
                        cells += extra;
                        let mut nodes = path.nodes.clone();
                        nodes.push(neighbor);
                        let mut edges = path.edges.clone();
                        edges.push(index);
                        queue.push_back(Pending { nodes, edges });
                        queued += 1;
                    }
                }
            }
        }
        // Bound the complete borrowed source report before duplicating raw edges.
        let borrowed = ReportRefs {
            source: &r.source,
            target: &r.target,
            paths: accepted
                .iter()
                .map(|p| PathRefs {
                    path: p.nodes.clone(),
                    edges: p.edges.iter().map(|&i| &rels[i]).collect(),
                    length: p.edges.len() as u64,
                })
                .collect(),
            num_paths: accepted.len() as u64,
        };
        crate::datalog_port::check_serialized(&borrowed, MAX_OUTPUT_BYTES - 1, true)?;
        let out = SemanticTemporalPathsResult {
            schema: "semantica.temporal-paths-result/1".into(),
            profile: "semantica.source-temporal-paths/1".into(),
            request_sha256: crate::sha256(
                semantica_contracts::json::canonical_json(
                    &serde_json::to_value(r).map_err(invalid)?,
                )
                .map_err(invalid)?,
            ),
            clock_at: time::text(clock),
            report: SemanticTemporalPathsReport {
                source: r.source.clone(),
                target: r.target.clone(),
                paths: accepted
                    .iter()
                    .map(|p| SemanticTemporalPath {
                        path: p.nodes.iter().map(|v| (*v).clone()).collect(),
                        edges: p.edges.iter().map(|&i| rels[i].clone()).collect(),
                        length: p.edges.len() as u64,
                    })
                    .collect(),
                num_paths: accepted.len() as u64,
            },
            path_relationship_indices: accepted
                .iter()
                .map(|p| p.edges.iter().map(|&i| i as u64).collect())
                .collect(),
            clock_fallback_count: fallbacks,
            edge_expansion_count: expansions,
            queued_path_count: queued,
            identity_comparison_count: identities.0,
        };
        crate::datalog_port::check_serialized(&out, MAX_OUTPUT_BYTES - 1, true)?;
        Ok(out)
    }
}
