//! Read-only Explorer operations over the sole ordered context projection.
//! HTTP admission/source binding belongs to the service, not graph records.
use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::URL_SAFE};
use semantica_contracts::{SemanticExplorerGraphReadQuery, SemanticExplorerGraphReadResult};
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    time::Instant,
};

fn within(deadline: Instant) -> Result<()> {
    ensure!(Instant::now() < deadline, "explorer_read_deadline_exceeded");
    Ok(())
}
pub(crate) fn truth(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::String(v) => !v.is_empty(),
        Value::Array(v) => !v.is_empty(),
        Value::Object(v) => !v.is_empty(),
        Value::Number(_) => !semantica_json::python_equal(v, &json!(0)).unwrap_or(false),
    }
}
fn string(v: &Value) -> Result<String> {
    Ok(semantica_json::python_str(v)?)
}
pub(crate) fn metadata(raw: &Value) -> Result<Map<String, Value>> {
    let mut out = Map::new();
    for key in ["metadata", "properties"] {
        if let Some(v) = raw.get(key).filter(|v| truth(v)) {
            for (k, v) in v.as_object().context("explorer_read_properties_invalid")? {
                out.insert(k.clone(), v.clone());
            }
        }
    }
    Ok(out)
}
pub(crate) fn dates(raw: &Value, meta: &mut Map<String, Value>) -> (Value, Value) {
    let from = raw
        .get("valid_from")
        .or_else(|| meta.get("valid_from"))
        .cloned()
        .unwrap_or(Value::Null);
    let until = raw
        .get("valid_until")
        .or_else(|| meta.get("valid_until"))
        .cloned()
        .unwrap_or(Value::Null);
    if !from.is_null() {
        meta.insert("valid_from".into(), from.clone());
    }
    if !until.is_null() {
        meta.insert("valid_until".into(), until.clone());
    }
    (from, until)
}
pub(crate) fn node(raw: &Value) -> Result<Value> {
    let mut meta = metadata(raw)?;
    let content = raw
        .get("content")
        .filter(|v| !v.is_null())
        .cloned()
        .unwrap_or_else(|| {
            [
                meta.get("content"),
                meta.get("text"),
                meta.get("label"),
                meta.get("name"),
                raw.get("label"),
                raw.get("name"),
            ]
            .into_iter()
            .flatten()
            .find(|v| truth(v))
            .cloned()
            .unwrap_or_else(|| raw.get("id").cloned().unwrap_or(json!("")))
        });
    meta.entry("content").or_insert(content.clone());
    let (from, until) = dates(raw, &mut meta);
    Ok(json!({"id":string(raw.get("id").unwrap_or(&json!("")))?,
        "type":string(raw.get("type").unwrap_or(&json!("entity")))?,
        "content":if truth(&content){string(&content)?}else{String::new()},
        "properties":meta,"valid_from":from,"valid_until":until}))
}
fn number(raw: &Value) -> Option<f64> {
    semantica_json::python_builtin_float(raw).ok()
}
fn edge(raw: &Value) -> Result<Value> {
    let mut meta = metadata(raw)?;
    let (from, until) = dates(raw, &mut meta);
    let id = raw
        .get("id")
        .and_then(Value::as_str)
        .context("context_edge_id_required")?;
    // The context projection already owns resolved, unique edge identities.
    let family = raw
        .get("familyId")
        .or_else(|| raw.get("family_id"))
        .or_else(|| meta.get("familyId"))
        .or_else(|| meta.get("family_id"))
        .filter(|v| truth(v))
        .map(string)
        .transpose()?
        .unwrap_or_else(|| id.into());
    let weight = raw.get("weight").and_then(number).unwrap_or(1.0);
    ensure!(weight.is_finite(), "explorer_read_nonfinite_response");
    Ok(
        json!({"id":id,"familyId":family,"source":raw["source"],"target":raw["target"],
        "type":string(raw.get("type").unwrap_or(&json!("related_to")))?,"weight":weight,
        "properties":meta,"valid_from":from,"valid_until":until}),
    )
}
fn ascii_string(s: &str) -> Result<String> {
    let encoded = serde_json::to_string(s)?;
    let mut out = String::new();
    for c in encoded.chars() {
        if c.is_ascii() {
            out.push(c);
        } else {
            for unit in c.encode_utf16(&mut [0; 2]) {
                use std::fmt::Write;
                write!(&mut out, "\\u{unit:04x}")?;
            }
        }
    }
    Ok(out)
}
// Original json.dumps defaults: ASCII escaping, insertion order and spaces.
fn search_json(v: &Value) -> Result<String> {
    Ok(match v {
        Value::String(s) => ascii_string(s)?,
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(_) => string(v)?,
        Value::Array(a) => format!(
            "[{}]",
            a.iter()
                .map(search_json)
                .collect::<Result<Vec<_>>>()?
                .join(", ")
        ),
        Value::Object(o) => format!(
            "{{{}}}",
            o.iter()
                .map(|(k, v)| Ok(format!("{}: {}", ascii_string(k)?, search_json(v)?)))
                .collect::<Result<Vec<_>>>()?
                .join(", ")
        ),
    })
}
fn cursor(raw: Option<&str>) -> Option<String> {
    let raw = raw?;
    if !raw.is_ascii() {
        return None;
    }
    // Match the original non-validating decoder, including ignored junk,
    // surplus padding and nonzero trailing bits. An incomplete group rejects.
    let mut out = Vec::new();
    let mut position = 0u8;
    let mut carry = 0u8;
    let mut padding = 0u8;
    let mut terminated = false;
    for b in raw.bytes() {
        if b == b'=' {
            if position >= 2 {
                padding += 1;
                if position + padding >= 4 {
                    terminated = true;
                    break;
                }
            }
            continue;
        }
        let value = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => continue,
        };
        padding = 0;
        match position {
            0 => carry = value << 2,
            1 => {
                out.push(carry | (value >> 4));
                carry = (value & 15) << 4;
            }
            2 => {
                out.push(carry | (value >> 2));
                carry = (value & 3) << 6;
            }
            _ => {
                out.push(carry | value);
                carry = 0;
            }
        }
        position = (position + 1) % 4;
    }
    if !terminated && position != 0 {
        return None;
    }
    String::from_utf8(out).ok().filter(|s| !s.is_empty())
}
fn bbox(raw: Option<&str>) -> Result<Option<[f64; 4]>> {
    let Some(raw) = raw.filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let values = raw
        .split(',')
        .map(|s| number(&json!(s)).context("explorer_read_bbox_invalid"))
        .collect::<Result<Vec<_>>>()?;
    ensure!(values.len() == 4, "explorer_read_bbox_invalid");
    ensure!(
        !(values[0] > values[2] || values[1] > values[3]),
        "explorer_read_bbox_invalid"
    );
    Ok(Some([values[0], values[1], values[2], values[3]]))
}
fn reply(status: u64, body: Value) -> SemanticExplorerGraphReadResult {
    SemanticExplorerGraphReadResult {
        schema: "semantica.explorer-graph-read-result/1".into(),
        status,
        body,
    }
}
pub(crate) fn validate_body(kind: &str, body: Value) -> Result<Value> {
    let schema =
        crate::schema::RustSchemaRuntime::new().map_err(|e| anyhow::anyhow!("{}", e.code))?;
    let result = schema
        .validate_model(&format!("semantica.explorer.schemas::{kind}"), &body)
        .map_err(|e| anyhow::anyhow!("{}", e.code))?;
    ensure!(
        result["status"] == "validated",
        "explorer_read_response_model_invalid"
    );
    Ok(result["output"].clone())
}

/// Cooperative checks discard expired results; the caller supplies any required
/// process preemption. No credentials, second graph, source claims or mutation.
pub fn execute(
    session: &crate::dataset::DatasetSession,
    q: &SemanticExplorerGraphReadQuery,
    deadline: Instant,
) -> Result<SemanticExplorerGraphReadResult> {
    within(deadline)?;
    ensure!(q.validate().is_ok(), "explorer_read_query_contract");
    ensure!(
        (1..=5000).contains(&q.limit) && (1..=5).contains(&q.depth),
        "explorer_read_query_bounds"
    );
    ensure!(
        serde_json::to_vec(q)?.len() <= session.dataset_byte_limit(),
        "explorer_read_query_budget"
    );
    ensure!(session.dataset().iter().any(|q| matches!(q.graph_name,oxigraph::model::GraphNameRef::NamedNode(n) if n.as_str()==crate::context::CONTEXT_GRAPH)),"context_projection_unavailable");
    let records = crate::context::records(session)?;
    within(deadline)?;
    if q.operation == "node" {
        let id = q
            .node_id
            .as_deref()
            .context("explorer_read_node_id_required")?;
        let Some(raw) = records.nodes.iter().find(|n| n["id"] == id) else {
            return Ok(reply(
                404,
                json!({"detail":format!("Node '{id}' not found")}),
            ));
        };
        let result = validate_body("NodeResponse", node(raw)?)?;
        within(deadline)?;
        return Ok(reply(200, result));
    }
    if q.operation == "neighbors" {
        let id = q
            .node_id
            .as_deref()
            .context("explorer_read_node_id_required")?;
        let nodes: BTreeMap<_, _> = records
            .nodes
            .iter()
            .map(|n| (n["id"].as_str().unwrap_or(""), n))
            .collect();
        let mut seen = BTreeSet::from([id]);
        let mut queue = VecDeque::from([(id, 0)]);
        let mut result = Vec::new();
        if nodes.contains_key(id) {
            while let Some((at, hop)) = queue.pop_front() {
                within(deadline)?;
                if hop >= q.depth {
                    continue;
                }
                for e in records.edges.iter().filter(|e| e["source"] == at) {
                    within(deadline)?;
                    let weight = e.get("weight").and_then(number).unwrap_or(1.0);
                    if weight < 0.0 {
                        continue;
                    }
                    ensure!(weight.is_finite(), "explorer_read_nonfinite_response");
                    let target = e["target"]
                        .as_str()
                        .context("context_edge_target_required")?;
                    if !seen.insert(target) {
                        continue;
                    }
                    queue.push_back((target, hop + 1));
                    if let Some(n) = nodes.get(target) {
                        let normalized = node(n)?;
                        result.push(json!({"id":target,"type":normalized["type"],
                    "content":normalized["content"],"relationship":e.get("type").unwrap_or(&json!("related_to")),"weight":weight,"hop":hop+1}));
                    }
                }
            }
        }
        let result = result
            .into_iter()
            .map(|v| validate_body("NeighborResponse", v))
            .collect::<Result<Vec<_>>>()?;
        within(deadline)?;
        return Ok(reply(200, json!(result)));
    }
    let nodes = q.operation == "nodes";
    let bounds = if nodes {
        match bbox(q.bbox.as_deref()) {
            Ok(b) => b,
            Err(_) => return Ok(reply(422, json!({"detail":"Invalid input"}))),
        }
    } else {
        None
    };
    let mut values = Vec::new();
    for raw in if nodes {
        &records.nodes
    } else {
        &records.edges
    } {
        within(deadline)?;
        let v = if nodes { node(raw)? } else { edge(raw)? };
        let kind = if nodes { &q.node_type } else { &q.edge_type };
        if kind
            .as_ref()
            .is_some_and(|t| !t.is_empty() && v["type"] != *t)
        {
            continue;
        }
        if nodes {
            if let Some(search) = q.search.as_deref().filter(|s| !s.is_empty()) {
                let lower = search.to_lowercase();
                let needle = semantica_json::python_strip(&lower);
                let haystacks = [
                    string(&v["id"])?,
                    string(&v["type"])?,
                    string(&v["content"])?,
                    search_json(&v["properties"])?,
                ];
                if !haystacks.iter().any(|s| s.to_lowercase().contains(needle)) {
                    continue;
                }
            }
            if let Some([a, b, c, d]) = bounds {
                let (Some(x), Some(y)) =
                    (number(&v["properties"]["x"]), number(&v["properties"]["y"]))
                else {
                    continue;
                };
                if !(a <= x && x <= c && b <= y && y <= d) {
                    continue;
                }
            }
        } else if q
            .source
            .as_ref()
            .is_some_and(|s| !s.is_empty() && v["source"] != *s)
            || q.target
                .as_ref()
                .is_some_and(|s| !s.is_empty() && v["target"] != *s)
        {
            continue;
        }
        values.push(v);
    }
    values.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    within(deadline)?;
    let skip: serde_json::Number =
        serde_json::from_str(&q.skip).context("explorer_read_skip_invalid")?;
    ensure!(
        !q.skip.is_empty() && q.skip.bytes().all(|b| b.is_ascii_digit()),
        "explorer_read_skip_invalid"
    );
    let total = values.len();
    let mut start = skip
        .as_u64()
        .and_then(|v| usize::try_from(v).ok())
        .unwrap_or(usize::MAX);
    if let Some(c) = cursor(q.cursor.as_deref()) {
        start = values
            .iter()
            .position(|n| n["id"] == c)
            .map_or(0, |i| i + 1);
    }
    let end = start.saturating_add(q.limit as usize).min(total);
    let page = if start < total {
        values[start..end].to_vec()
    } else {
        Vec::new()
    };
    let next = if end < total && !page.is_empty() {
        Some(URL_SAFE.encode(page.last().unwrap()["id"].as_str().unwrap()))
    } else {
        None
    };
    let key = if nodes { "nodes" } else { "edges" };
    let body = json!({key:page,"total":total,"skip":skip,"limit":q.limit,"next_cursor":next,"has_more":next.is_some()});
    let body = validate_body(
        if nodes {
            "NodeListResponse"
        } else {
            "EdgeListResponse"
        },
        body,
    )?;
    within(deadline)?;
    Ok(reply(200, body))
}

#[cfg(test)]
mod cursor_tests {
    #[test]
    fn original_python_decoder_exhaustive_short_inputs() {
        let cases: Vec<(String, Option<String>)> = serde_json::from_str(include_str!(
            "../tests/fixtures/explorer_cursor_python.json"
        ))
        .unwrap();
        assert_eq!(cases.len(), 19608);
        for (input, expected) in cases {
            assert_eq!(super::cursor(Some(&input)), expected, "{input:?}");
        }
        assert_eq!(super::cursor(Some("6L20")), Some("轴".into()));
        assert_eq!(super::cursor(Some("轴")), None);
    }
}
mod native;
pub(crate) use native::is_original_normalization_error;
pub(crate) use native::normalize_search_results_native;
pub use native::{NativeNodeProjection, project_native_node_records};
pub(crate) use native::{normalize_native_state_nodes, normalize_native_state_search_results};
pub use native::{normalize_node_native, normalize_nodes_native};
