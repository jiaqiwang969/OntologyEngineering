//! Original greedy pattern detection with explicit, ordered session state.
//! Structural recurrence is not evidence of causal or engineering validity.
use crate::{runtime::RustSemanticRuntime, temporal_intervals as time};
use chrono::{NaiveDate, NaiveDateTime};
use semantica_contracts::{
    SemanticRuntimeFailure, SemanticTemporalPatternAction, SemanticTemporalPatternStep,
    SemanticTemporalPatternsPort, SemanticTemporalPatternsRequest, SemanticTemporalPatternsResult,
};
use serde::Serialize;
use serde_json::{Map, Value, json};

pub const MAX_INPUT_BYTES: usize = 1024 * 1024;
pub const MAX_OUTPUT_BYTES: usize = 16 * 1024 * 1024;
type Result<T> = std::result::Result<T, SemanticRuntimeFailure>;
fn error(code: &str, message: impl ToString) -> SemanticRuntimeFailure {
    SemanticRuntimeFailure {
        code: format!("temporal_patterns_{code}"),
        phase: "execution".into(),
        message: message.to_string(),
        retryable: false,
    }
}
fn invalid(message: impl ToString) -> SemanticRuntimeFailure {
    error("invalid", message)
}
fn resource(message: &str) -> SemanticRuntimeFailure {
    error("resource_limit", message)
}
fn source(kind: &str, message: impl ToString) -> SemanticRuntimeFailure {
    error("source_failure", format!("{kind}: {}", message.to_string()))
}
fn kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "NoneType",
        Value::Bool(_) => "bool",
        Value::Number(n) if n.is_f64() => "float",
        Value::Number(_) => "int",
        Value::String(_) => "str",
        Value::Array(_) => "list",
        Value::Object(_) => "dict",
    }
}
fn object(v: &Value) -> Result<&Map<String, Value>> {
    v.as_object().ok_or_else(|| {
        source(
            "builtins.AttributeError",
            format!("'{}' object has no attribute 'get'", kind(v)),
        )
    })
}
fn hashable(v: &Value) -> Result<()> {
    if v.is_array() || v.is_object() {
        return Err(source(
            "builtins.TypeError",
            format!("unhashable type: '{}'", kind(v)),
        ));
    }
    Ok(())
}
#[derive(Default)]
struct Work {
    clock: u64,
    visits: u64,
    comparisons: u64,
    cells: usize,
}
impl Work {
    fn equal(&mut self, a: &Value, b: &Value) -> Result<bool> {
        if self.comparisons == 16_777_216 {
            return Err(resource("More than16777216 identity comparisons in action"));
        }
        self.comparisons += 1;
        semantica_json::python_equal(a, b).map_err(invalid)
    }
    fn visit(&mut self) -> Result<()> {
        if self.visits == 65536 {
            return Err(resource("More than65536 edge visits in action"));
        }
        self.visits += 1;
        Ok(())
    }
    fn cells(&mut self, n: usize) -> Result<()> {
        self.cells = self.cells.saturating_add(n);
        if self.cells > 1_048_576 {
            return Err(resource("More than1048576 cumulative path cells in action"));
        }
        Ok(())
    }
}
fn bounds(
    f: &Map<String, Value>,
    clock: NaiveDateTime,
    w: &mut Work,
) -> Result<(Option<NaiveDateTime>, Option<NaiveDateTime>)> {
    crate::temporal_paths::bounds(f, clock, &mut w.clock).map_err(|mut e| {
        e.code = if e.code == "temporal_paths_source_failure" {
            "temporal_patterns_source_failure"
        } else {
            "temporal_patterns_invalid"
        }
        .into();
        e
    })
}
fn endpoint<'a>(f: &'a Map<String, Value>, key: &str) -> &'a Value {
    f.get(key).unwrap_or(&Value::Null)
}
fn gap_us(v: Option<&Value>) -> Result<i128> {
    const DAY: i128 = 86_400_000_000;
    let total = match v {
        Some(Value::Bool(b)) => i128::from(*b) * DAY,
        Some(Value::Number(n)) if n.is_i64() => i128::from(n.as_i64().unwrap()) * DAY,
        Some(Value::Number(n)) if n.is_u64() => i128::from(n.as_u64().unwrap()) * DAY,
        Some(Value::Number(n)) => {
            let d = n
                .as_f64()
                .ok_or_else(|| invalid("Gap must be a finite JSON number"))?;
            if !d.is_finite() {
                return Err(invalid("Gap must be a finite JSON number"));
            }
            // CPython accum splits whole days before multiplying the fractional part.
            // The factor is even, so final microsecond ties use the fraction's parity.
            if d.abs() > 2_147_483_648.0 {
                return Err(source(
                    "builtins.OverflowError",
                    "Python int too large to convert to C int",
                ));
            }
            (d.trunc() as i128) * DAY + (d.fract() * DAY as f64).round_ties_even() as i128
        }
        _ => 0,
    };
    let days = total.div_euclid(DAY);
    if days < i128::from(i32::MIN) || days > i128::from(i32::MAX) {
        return Err(source(
            "builtins.OverflowError",
            "Python int too large to convert to C int",
        ));
    }
    if !(-999_999_999..=999_999_999).contains(&days) {
        return Err(source(
            "builtins.OverflowError",
            format!("days={days}; must have magnitude <= 999999999"),
        ));
    }
    Ok(total)
}
fn enough(count: usize, min: &Value) -> Result<bool> {
    match min {
        Value::Bool(b) => Ok(count >= usize::from(*b)),
        Value::Number(n) => Ok((count as f64)
            >= n.as_f64()
                .ok_or_else(|| invalid("Support is not a finite JSON number"))?),
        _ => Err(source(
            "builtins.TypeError",
            format!(
                "'>=' not supported between instances of 'int' and '{}'",
                kind(min)
            ),
        )),
    }
}
struct Adjacency<'a> {
    source: &'a Value,
    edges: Vec<usize>,
}
#[derive(Serialize)]
struct Occurrence<'a> {
    nodes: Vec<&'a Value>,
    edges: Vec<&'a Value>,
    start_time: Option<String>,
    end_time: Option<String>,
}
#[derive(Serialize)]
struct Pattern<'a> {
    pattern_type: &'a str,
    signature: Vec<&'a Value>,
    frequency: usize,
    occurrences: Vec<Occurrence<'a>>,
}
struct Group<'a> {
    pattern: Pattern<'a>,
    positions: Vec<Vec<u64>>,
}
fn same_signature(a: &[&Value], b: &[&Value], w: &mut Work) -> Result<bool> {
    if a.len() != b.len() {
        return Ok(false);
    }
    for (x, y) in a.iter().zip(b) {
        if !w.equal(x, y)? {
            return Ok(false);
        }
    }
    Ok(true)
}
fn record<'a>(
    groups: &mut Vec<Group<'a>>,
    nodes: Vec<&'a Value>,
    ids: &[usize],
    rels: &'a [Value],
    label: &'a str,
    clock: NaiveDateTime,
    w: &mut Work,
) -> Result<()> {
    for n in &nodes {
        hashable(n)?;
    }
    let mut group = None;
    for (i, g) in groups.iter().enumerate() {
        if same_signature(&g.pattern.signature, &nodes, w)? {
            group = Some(i);
            break;
        }
    }
    // Hashing the key occurs before constructing the value in the original.
    let (start, _) = bounds(object(&rels[ids[0]])?, clock, w)?;
    let (last_start, last_end) = bounds(object(&rels[*ids.last().unwrap()])?, clock, w)?;
    let occ = Occurrence {
        nodes: nodes.clone(),
        edges: ids.iter().map(|i| &rels[*i]).collect(),
        start_time: start.map(time::text),
        end_time: last_end.or(last_start).map(time::text),
    };
    let index = if let Some(i) = group {
        i
    } else {
        groups.push(Group {
            pattern: Pattern {
                pattern_type: label,
                signature: nodes,
                frequency: 0,
                occurrences: Vec::new(),
            },
            positions: Vec::new(),
        });
        groups.len() - 1
    };
    groups[index].pattern.frequency += 1;
    groups[index].pattern.occurrences.push(occ);
    groups[index]
        .positions
        .push(ids.iter().map(|i| *i as u64).collect());
    // Before cloning raw records, reject an oversized accumulated representation.
    crate::datalog_port::check_serialized(
        &groups.iter().map(|g| &g.pattern).collect::<Vec<_>>(),
        MAX_OUTPUT_BYTES,
        true,
    )
    .map_err(|_| resource("Pattern output exceeds16MiB before materialization"))?;
    Ok(())
}
fn relationships(v: Option<&Value>) -> Result<&[Value]> {
    match v {
        None => Ok(&[]),
        Some(Value::Array(a)) => Ok(a),
        Some(Value::String(s)) if s.is_empty() => Ok(&[]),
        Some(Value::Object(m)) if m.is_empty() => Ok(&[]),
        Some(Value::String(_) | Value::Object(_)) => Err(source(
            "builtins.AttributeError",
            "'str' object has no attribute 'get'",
        )),
        Some(v) => Err(source(
            "builtins.TypeError",
            format!("'{}' object is not iterable", kind(v)),
        )),
    }
}
fn patterns<'a>(
    args: &'a Map<String, Value>,
    label: &'a str,
    config: &Map<String, Value>,
    min: &Value,
    clock: NaiveDateTime,
    w: &mut Work,
) -> Result<Vec<Group<'a>>> {
    let graph = object(&args["graph"])?;
    if label != "sequence" && label != "cycle" {
        return Ok(Vec::new());
    }
    // Source converts the sequence gap before it attempts to iterate relationships.
    let gap = if label == "sequence" {
        gap_us(config.get("gap_tolerance"))?
    } else {
        0
    };
    let rels = relationships(graph.get("relationships"))?;
    if rels.len() > 4096 {
        return Err(resource("More than4096 relationships in action"));
    }
    let mut adjacency: Vec<Adjacency<'_>> = Vec::new();
    for (i, r) in rels.iter().enumerate() {
        let s = endpoint(object(r)?, "source");
        hashable(s)?;
        let mut found = None;
        for (k, a) in adjacency.iter().enumerate() {
            if w.equal(a.source, s)? {
                found = Some(k);
                break;
            }
        }
        if let Some(k) = found {
            adjacency[k].edges.push(i)
        } else {
            adjacency.push(Adjacency {
                source: s,
                edges: vec![i],
            });
        }
    }
    let mut groups = Vec::new();
    for (first, r) in rels.iter().enumerate() {
        w.visit()?;
        w.cells(3)?;
        let f = object(r)?;
        let start_node = endpoint(f, "source");
        let mut nodes = vec![start_node, endpoint(f, "target")];
        let mut ids = vec![first];
        let mut current = first;
        loop {
            if label == "cycle" {
                if ids.len() > 6 {
                    break;
                }
                if w.equal(nodes.last().unwrap(), start_node)? {
                    record(&mut groups, nodes.clone(), &ids, rels, label, clock, w)?;
                    break;
                }
            }
            let (_, end) = bounds(object(&rels[current])?, clock, w)?;
            if label == "sequence" && end.is_none() {
                break;
            }
            let target = endpoint(object(&rels[current])?, "target");
            hashable(target)?;
            let mut outgoing: &[usize] = &[];
            for a in &adjacency {
                if w.equal(a.source, target)? {
                    outgoing = &a.edges;
                    break;
                }
            }
            let mut chosen: Option<(Option<NaiveDateTime>, usize)> = None;
            for &i in outgoing {
                w.visit()?;
                if label == "sequence" && i == current {
                    continue;
                }
                let (s, _) = bounds(object(&rels[i])?, clock, w)?;
                if label == "sequence" {
                    let (Some(e), Some(s)) = (end, s) else {
                        continue;
                    };
                    let delta =
                        i128::from((s - e).num_microseconds().ok_or_else(|| {
                            invalid("Datetime difference outside transport range")
                        })?);
                    if delta < 0 || delta > gap {
                        continue;
                    }
                } else if let (Some(e), Some(s)) = (end, s) {
                    if s < e {
                        continue;
                    }
                }
                // None sorts as datetime.min; retain original order on equal keys.
                let minimum = NaiveDate::from_ymd_opt(1, 1, 1)
                    .unwrap()
                    .and_hms_opt(0, 0, 0)
                    .unwrap();
                if chosen.is_none_or(|(old, _)| s.unwrap_or(minimum) < old.unwrap_or(minimum)) {
                    chosen = Some((s, i));
                }
            }
            let Some((_, next)) = chosen else { break };
            let target = endpoint(object(&rels[next])?, "target");
            if label == "cycle" {
                let mut visited = false;
                for n in &nodes[1..nodes.len() - 1] {
                    if w.equal(n, target)? {
                        visited = true;
                        break;
                    }
                }
                if visited && !w.equal(target, start_node)? {
                    break;
                }
            }
            w.cells(2)?;
            current = next;
            ids.push(next);
            nodes.push(target);
        }
        if label == "sequence" && ids.len() >= 2 {
            record(&mut groups, nodes, &ids, rels, label, clock, w)?;
        }
    }
    let mut filtered = Vec::new();
    for g in groups {
        if enough(g.pattern.frequency, min)? {
            filtered.push(g);
        }
    }
    Ok(filtered)
}
fn action(
    act: &SemanticTemporalPatternAction,
    config: &mut Map<String, Value>,
    clock: NaiveDateTime,
    w: &mut Work,
) -> Result<(Value, Vec<Vec<Vec<u64>>>)> {
    let args = act
        .arguments
        .as_object()
        .ok_or_else(|| invalid("Action arguments must be an object"))?;
    let query = act.entry == "query";
    let key = if query { "pattern" } else { "pattern_type" };
    let label = args
        .get(key)
        .map(|v| {
            v.as_str()
                .ok_or_else(|| invalid("Pattern label must be a string"))
        })
        .transpose()?
        .unwrap_or("sequence");
    if let Some(gap) = args.get("gap_tolerance") {
        config.insert("gap_tolerance".into(), gap.clone());
    }
    let default = json!(if query { 1 } else { 2 });
    let min = args
        .get(if query {
            "min_support"
        } else {
            "min_frequency"
        })
        .unwrap_or(&default);
    let groups = patterns(args, label, config, min, clock, w)?;
    let refs = groups.iter().map(|g| &g.pattern).collect::<Vec<_>>();
    crate::datalog_port::check_serialized(&refs, MAX_OUTPUT_BYTES, true)
        .map_err(|_| resource("Pattern output exceeds16MiB before materialization"))?;
    let out = serde_json::to_value(refs).map_err(invalid)?;
    let positions = groups.into_iter().map(|g| g.positions).collect();
    Ok((
        if query {
            let n = out.as_array().unwrap().len();
            json!({"pattern":label,"patterns":out,"num_patterns":n})
        } else {
            out
        },
        positions,
    ))
}
impl SemanticTemporalPatternsPort for RustSemanticRuntime {
    fn temporal_patterns(
        &self,
        r: &SemanticTemporalPatternsRequest,
    ) -> Result<SemanticTemporalPatternsResult> {
        crate::datalog_port::check_serialized(r, MAX_INPUT_BYTES, false)
            .map_err(|_| invalid("Complete request exceeds1MiB"))?;
        if r.schema != "semantica.temporal-patterns-request/1" || r.actions.len() > 128 {
            return Err(invalid("Unknown schema or more than128 actions"));
        }
        let mut config = r
            .initial_config
            .as_object()
            .ok_or_else(|| invalid("Initial config must be an object"))?
            .clone();
        // Explicit transport validation precedes any action; it does not claim a Python exception.
        for act in &r.actions {
            let args = act
                .arguments
                .as_object()
                .ok_or_else(|| invalid("Action arguments must be an object"))?;
            if !matches!(act.entry.as_str(), "detector" | "query")
                || !args.contains_key("graph")
                || (act.entry == "query"
                    && (!args.contains_key("pattern")
                        || args.contains_key("pattern_type")
                        || args.contains_key("min_frequency")))
            {
                return Err(invalid(
                    "Action entry/required arguments or duplicate query bindings outside profile",
                ));
            }
        }
        let clock = time::timestamp(&r.clock_at).map_err(|e| invalid(e.message))?;
        let mut out = SemanticTemporalPatternsResult {
            schema: "semantica.temporal-patterns-result/1".into(),
            profile: "semantica.source-temporal-patterns/1".into(),
            request_sha256: crate::sha256(
                semantica_contracts::json::canonical_json(
                    &serde_json::to_value(r).map_err(invalid)?,
                )
                .map_err(invalid)?,
            ),
            clock_at: time::text(clock),
            steps: Vec::new(),
            final_config: Value::Object(config.clone()),
        };
        for (i, act) in r.actions.iter().enumerate() {
            let mut w = Work::default();
            let (result, positions, failure) = match action(act, &mut config, clock, &mut w) {
                Ok((r, p)) => (Some(r), p, None),
                Err(e) => (None, Vec::new(), Some(e)),
            };
            out.steps.push(SemanticTemporalPatternStep {
                index: i as u64,
                entry: act.entry.clone(),
                result,
                failure,
                config_after: Value::Object(config.clone()),
                occurrence_relationship_indices: positions,
                clock_fallback_count: w.clock,
                edge_visit_count: w.visits,
                identity_comparison_count: w.comparisons,
            });
            out.final_config = Value::Object(config.clone());
            crate::datalog_port::check_serialized(&out, MAX_OUTPUT_BYTES - 1, true)
                .map_err(|_| invalid("Complete session output exceeds16MiB"))?;
        }
        Ok(out)
    }
}
