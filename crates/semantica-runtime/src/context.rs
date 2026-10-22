//! ContextGraph record operations on the existing DatasetSession. No parallel state.
pub use crate::context_native::{read_native_context_state, store_native_context_state};
pub use crate::context_native::{read_native_records, store_native_records};
use crate::dataset::DatasetSession;
use anyhow::{Context, Result, bail, ensure};
use oxigraph::{
    io::RdfFormat,
    model::{GraphNameRef, Literal, NamedNode, Quad, TermRef},
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
pub(crate) const CONTEXT_GRAPH: &str = "urn:semantica:context:records/1";
const ROOT: &str = "urn:semantica:context:root";
const SCHEMA: &str = "urn:semantica:context:schema";
const NODE_ID: &str = "urn:semantica:context:nodeId";
const NODE_TYPE: &str = "urn:semantica:context:nodeType";
const NODE_RECORD: &str = "urn:semantica:context:nodeRecord";
const EDGE_RECORD: &str = "urn:semantica:context:edgeRecord";
const CONTEXT_PROFILE: &str = "semantica.context-record-projection/1";
const ORDER: &str = "urn:semantica:context:recordOrder";
const MUTABLE_PROFILE: &str = "semantica.context-record-projection/2";
fn encoded_quad(
    subject: NamedNode,
    predicate: &str,
    object: Literal,
    graph: &NamedNode,
) -> Result<String> {
    Ok(format!(
        "{} .\n",
        Quad::new(subject, NamedNode::new(predicate)?, object, graph.clone())
    ))
}
/// Explicit, finite ContextGraph record view in the SAME dataset. No RDF subject counting.
/// Only a trusted host calls this loader; wire requests never contain graph/adoption proofs.
/// All records are validated before the existing staged DatasetSession loader publishes once.
pub fn load_context_graph(session: &mut DatasetSession, input: &Value) -> Result<()> {
    ensure!(
        serde_json::to_vec(input)?.len() <= session.dataset_byte_limit(),
        "context_record_byte_budget"
    );
    let nodes = input
        .get("nodes")
        .and_then(Value::as_array)
        .context("context_nodes_required")?;
    let edges = input
        .get("edges")
        .and_then(Value::as_array)
        .context("context_edges_required")?;
    ensure!(
        nodes.len() <= session.context_node_limit() && edges.len() <= 50_000,
        "context_record_count_budget"
    );
    ensure!(
        !session
            .dataset()
            .iter()
            .any(|q| q.graph_name.to_string() == format!("<{CONTEXT_GRAPH}>")),
        "context_projection_already_loaded"
    );
    let graph = NamedNode::new(CONTEXT_GRAPH)?;
    let mut data = String::new();
    let mut typed = session
        .typed_context_commit()
        .then(oxigraph::model::Dataset::new);
    let mut push = |subject: NamedNode, predicate: &str, object: Literal| -> Result<()> {
        if let Some(dataset) = &mut typed {
            dataset.insert(&Quad::new(
                subject,
                NamedNode::new(predicate)?,
                object,
                graph.clone(),
            ));
        } else {
            data.push_str(&encoded_quad(subject, predicate, object, &graph)?);
        }
        Ok(())
    };
    push(
        NamedNode::new(ROOT)?,
        SCHEMA,
        Literal::new_simple_literal(MUTABLE_PROFILE),
    )?;
    let mut node_ids = BTreeSet::new();
    for (position, node) in nodes.iter().enumerate() {
        let id = node
            .get("id")
            .and_then(Value::as_str)
            .context("context_node_id_required")?;
        ensure!(
            !id.is_empty() && id.len() <= 512 && node_ids.insert(id.to_owned()),
            "context_node_identity_invalid"
        );
        let kind = node
            .get("type")
            .map(|v| v.as_str().context("context_node_type_invalid"))
            .transpose()?
            .unwrap_or("entity");
        ensure!(
            !kind.is_empty() && kind.len() <= 512,
            "context_node_type_invalid"
        );
        let record = NamedNode::new(format!("urn:semantica:context:node:{}", crate::sha256(id)))?;
        push(record.clone(), NODE_ID, Literal::new_simple_literal(id))?;
        push(record.clone(), NODE_TYPE, Literal::new_simple_literal(kind))?;
        push(
            record.clone(),
            NODE_RECORD,
            Literal::new_simple_literal(serde_json::to_string(node)?),
        )?;
        push(
            record,
            ORDER,
            Literal::new_simple_literal(position.to_string()),
        )?;
    }
    let mut edge_ids = BTreeSet::new();
    for (position, edge) in edges.iter().enumerate() {
        let id = edge
            .get("id")
            .and_then(Value::as_str)
            .context("context_edge_id_required")?;
        let source = edge
            .get("source")
            .and_then(Value::as_str)
            .context("context_edge_source_required")?;
        let target = edge
            .get("target")
            .and_then(Value::as_str)
            .context("context_edge_target_required")?;
        ensure!(
            !id.is_empty() && id.len() <= 512 && edge_ids.insert(id.to_owned()),
            "context_edge_identity_invalid"
        );
        ensure!(
            node_ids.contains(source) && node_ids.contains(target),
            "context_edge_endpoint_missing"
        );
        let record = NamedNode::new(format!("urn:semantica:context:edge:{}", crate::sha256(id)))?;
        push(
            record.clone(),
            EDGE_RECORD,
            Literal::new_simple_literal(serde_json::to_string(edge)?),
        )?;
        push(
            record,
            ORDER,
            Literal::new_simple_literal(position.to_string()),
        )?;
    }
    if let Some(dataset) = typed {
        session.replace_named_graph_owned(CONTEXT_GRAPH, dataset)?;
    } else {
        session.load(data.as_bytes(), RdfFormat::NQuads, None, None)?;
    }
    Ok(())
}

pub fn context_summary(session: &DatasetSession) -> Result<Value> {
    context_summary_with_records(session, None)
}
// Transaction-local parsed records: validation and projection consume the same
// JSON values, without retaining a second authority or parsing each value twice.
fn context_summary_with_records(
    session: &DatasetSession,
    mut captured: Option<&mut BTreeMap<String, Value>>,
) -> Result<Value> {
    if session.quad_count() == 0 {
        return Ok(json!({"node_count":0,"decision_count":0,"graph_ready":true}));
    }
    let mut schema_count = 0;
    let mut nodes = BTreeMap::<String, BTreeMap<String, String>>::new();
    let mut edges = BTreeMap::<String, String>::new();
    for quad in session.dataset().iter() {
        if !matches!(quad.graph_name,GraphNameRef::NamedNode(n)if n.as_str()==CONTEXT_GRAPH) {
            continue;
        }
        let subject = quad.subject.to_string();
        let TermRef::Literal(object) = quad.object else {
            bail!("context_projection_nonliteral");
        };
        ensure!(
            object.language().is_none()
                && object.datatype().as_str() == "http://www.w3.org/2001/XMLSchema#string",
            "context_projection_literal_identity_mismatch"
        );
        match quad.predicate.as_str() {
            SCHEMA => {
                ensure!(
                    subject == format!("<{ROOT}>")
                        && [CONTEXT_PROFILE, MUTABLE_PROFILE].contains(&object.value()),
                    "context_projection_schema_invalid"
                );
                schema_count += 1;
            }
            NODE_ID | NODE_TYPE | NODE_RECORD => {
                ensure!(
                    nodes
                        .entry(subject)
                        .or_default()
                        .insert(quad.predicate.as_str().into(), object.value().into())
                        .is_none(),
                    "context_projection_duplicate_property"
                );
            }
            EDGE_RECORD => {
                ensure!(
                    edges.insert(subject, object.value().into()).is_none(),
                    "context_projection_duplicate_edge"
                );
            }
            ORDER => {
                ensure!(
                    object.value().parse::<usize>().is_ok(),
                    "context_projection_order_invalid"
                );
            }
            _ => bail!("context_projection_unknown_property"),
        }
    }
    ensure!(schema_count == 1, "context_projection_unavailable");
    let mut ids = BTreeSet::new();
    let mut decisions = 0;
    for (subject, fields) in &nodes {
        ensure!(fields.len() == 3, "context_projection_incomplete_node");
        let id = fields
            .get(NODE_ID)
            .context("context_projection_missing_id")?;
        let kind = fields
            .get(NODE_TYPE)
            .context("context_projection_missing_type")?;
        let raw: Value = crate::providers::parse_protocol_json(
            fields
                .get(NODE_RECORD)
                .context("context_projection_missing_record")?,
        )
        .map_err(|_| anyhow::anyhow!("context_projection_invalid_json"))?;
        ensure!(
            !id.is_empty()
                && ids.insert(id.clone())
                && subject == &format!("<urn:semantica:context:node:{}>", crate::sha256(id)),
            "context_projection_identity_mismatch"
        );
        ensure!(
            raw["id"].as_str() == Some(id.as_str())
                && raw.get("type").and_then(Value::as_str).unwrap_or("entity") == kind,
            "context_projection_record_mismatch"
        );
        if kind == "decision" {
            decisions += 1;
        }
        if let Some(records) = captured.as_deref_mut() {
            records.insert(subject.clone(), raw);
        }
    }
    let mut edge_ids = BTreeSet::new();
    for (subject, raw) in edges {
        let raw: Value = crate::providers::parse_protocol_json(&raw)
            .map_err(|_| anyhow::anyhow!("context_projection_invalid_json"))?;
        let id = raw["id"]
            .as_str()
            .context("context_projection_edge_id_missing")?;
        ensure!(
            edge_ids.insert(id.to_owned())
                && subject == format!("<urn:semantica:context:edge:{}>", crate::sha256(id)),
            "context_projection_edge_identity_mismatch"
        );
        ensure!(
            raw["source"].as_str().is_some_and(|s| ids.contains(s))
                && raw["target"].as_str().is_some_and(|s| ids.contains(s)),
            "context_projection_edge_endpoint_missing"
        );
        if let Some(records) = captured.as_deref_mut() {
            records.insert(subject, raw);
        }
    }
    Ok(json!({"node_count":nodes.len(),"decision_count":decisions,"graph_ready":true}))
}

/// A transaction-local projection. DatasetSession remains the sole state owner.
#[derive(Default)]
pub(crate) struct Records {
    pub(crate) nodes: Vec<Value>,
    pub(crate) edges: Vec<Value>,
}
pub(crate) fn records(session: &DatasetSession) -> Result<Records> {
    if !session
        .dataset()
        .iter()
        .any(|q| matches!(q.graph_name,GraphNameRef::NamedNode(n)if n.as_str()==CONTEXT_GRAPH))
    {
        return Ok(Records::default());
    }
    let mut validated = BTreeMap::new();
    context_summary_with_records(session, Some(&mut validated))?;
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut orders = BTreeMap::new();
    for q in session.dataset().iter() {
        if !matches!(q.graph_name,GraphNameRef::NamedNode(n)if n.as_str()==CONTEXT_GRAPH) {
            continue;
        }
        let TermRef::Literal(value) = q.object else {
            bail!("context_projection_nonliteral")
        };
        match q.predicate.as_str() {
            NODE_RECORD | EDGE_RECORD => {
                let raw = validated
                    .remove(&q.subject.to_string())
                    .context("context_projection_record_missing")?;
                if q.predicate.as_str() == NODE_RECORD {
                    nodes.push((q.subject.to_string(), raw));
                } else {
                    edges.push((q.subject.to_string(), raw));
                }
            }
            ORDER => {
                ensure!(
                    orders
                        .insert(q.subject.to_string(), value.value().parse::<usize>()?)
                        .is_none(),
                    "context_projection_duplicate_order"
                );
            }
            _ => {}
        }
    }
    if !orders.is_empty() {
        ensure!(
            orders.len() == nodes.len() + edges.len(),
            "context_projection_incomplete_order"
        );
        for items in [&mut nodes, &mut edges] {
            let mut seen = BTreeSet::new();
            for (subject, _) in items.iter() {
                ensure!(
                    orders
                        .get(subject)
                        .is_some_and(|n| *n < items.len() && seen.insert(*n)),
                    "context_projection_order_invalid"
                );
            }
            items.sort_by_key(|(subject, _)| orders[subject]);
        }
    }
    Ok(Records {
        nodes: nodes.into_iter().map(|(_, v)| v).collect(),
        edges: edges.into_iter().map(|(_, v)| v).collect(),
    })
}
pub(crate) fn commit(session: &mut DatasetSession, data: &Records) -> Result<()> {
    commit_records(session, data, false)
}
/// Original dictionary insertion order is observable during Explorer index rebuild.
pub(crate) fn commit_original(session: &mut DatasetSession, data: &Records) -> Result<()> {
    commit_records(session, data, true)
}
fn commit_records(
    session: &mut DatasetSession,
    data: &Records,
    preserve_order: bool,
) -> Result<()> {
    let encode = |value: &Value| -> Result<String> {
        if preserve_order {
            Ok(serde_json::to_string(value)?)
        } else {
            Ok(String::from_utf8(semantica_json::canonical_json(value)?)?)
        }
    };
    ensure!(
        data.nodes.len() <= session.context_node_limit() && data.edges.len() <= 50_000,
        "context_record_count_budget"
    );
    // The existing staged update path parses/evaluates once and publishes once.
    // Typed RDF terms prevent identifiers/JSON from becoming update syntax.
    let mut triples = String::new();
    let mut typed = session
        .typed_context_commit()
        .then(oxigraph::model::Dataset::new);
    let graph = NamedNode::new(CONTEXT_GRAPH)?;
    let mut push = |s: NamedNode, p: &str, o: Literal| -> Result<()> {
        if let Some(dataset) = &mut typed {
            dataset.insert(&Quad::new(s, NamedNode::new(p)?, o, graph.clone()));
        } else {
            triples.push_str(&format!("{} {} {} .\n", s, NamedNode::new(p)?, o));
        }
        Ok(())
    };
    push(
        NamedNode::new(ROOT)?,
        SCHEMA,
        Literal::new_simple_literal(MUTABLE_PROFILE),
    )?;
    for (order, node) in data.nodes.iter().enumerate() {
        let id = node["id"].as_str().context("context_node_id_required")?;
        let subject = NamedNode::new(format!("urn:semantica:context:node:{}", crate::sha256(id)))?;
        push(subject.clone(), NODE_ID, Literal::new_simple_literal(id))?;
        push(
            subject.clone(),
            NODE_TYPE,
            Literal::new_simple_literal(node["type"].as_str().unwrap_or("entity")),
        )?;
        push(
            subject.clone(),
            NODE_RECORD,
            Literal::new_simple_literal(encode(node)?),
        )?;
        push(
            subject,
            ORDER,
            Literal::new_simple_literal(order.to_string()),
        )?;
    }
    for (order, edge) in data.edges.iter().enumerate() {
        let id = edge["id"].as_str().context("context_edge_id_required")?;
        let subject = NamedNode::new(format!("urn:semantica:context:edge:{}", crate::sha256(id)))?;
        push(
            subject.clone(),
            EDGE_RECORD,
            Literal::new_simple_literal(encode(edge)?),
        )?;
        push(
            subject,
            ORDER,
            Literal::new_simple_literal(order.to_string()),
        )?;
    }
    if let Some(dataset) = typed {
        session.replace_named_graph_owned(CONTEXT_GRAPH, dataset)?;
    } else {
        ensure!(
            triples.len() <= crate::MAX_BYTES,
            "context_record_byte_budget"
        );
        session.update(&format!("DROP SILENT GRAPH <{CONTEXT_GRAPH}>; INSERT DATA {{ GRAPH <{CONTEXT_GRAPH}> {{ {triples} }} }}"))?;
    }
    context_summary(session)?;
    Ok(())
}
fn add_node(
    data: &mut Records,
    id: &str,
    kind: &str,
    content: &str,
    properties: Value,
) -> Result<bool> {
    // Original skips whitespace ids even though the canonical tool returns added.
    if id.trim().is_empty() {
        return Ok(false);
    }
    ensure!(
        id.len() <= 512 && !kind.is_empty() && kind.len() <= 512,
        "context_node_identity_invalid"
    );
    let node = json!({"id":id,"type":kind,"content":content,"properties":properties});
    if let Some(existing) = data.nodes.iter_mut().find(|n| n["id"] == id) {
        *existing = node;
    } else {
        data.nodes.push(node);
    }
    Ok(true)
}
fn ascii_json(value: &Value) -> Result<Vec<u8>> {
    let raw = String::from_utf8(semantica_json::canonical_json(value)?)?;
    let mut result = String::new();
    for c in raw.chars() {
        if c.is_ascii() {
            result.push(c)
        } else {
            let mut units = [0; 2];
            for u in c.encode_utf16(&mut units).iter() {
                result.push_str(&format!("\\u{u:04x}"));
            }
        }
    }
    Ok(result.into_bytes())
}
/// Original content-derived UUID5(URL, Python sorted/ASCII compact JSON).
pub(crate) fn edge_identity(
    source: &str,
    target: &str,
    kind: &str,
    weight: f64,
    properties: &Value,
) -> Result<String> {
    edge_identity_with_validity(
        source,
        target,
        kind,
        weight,
        properties,
        &Value::Null,
        &Value::Null,
    )
}
pub(crate) fn edge_identity_with_validity(
    source: &str,
    target: &str,
    kind: &str,
    weight: f64,
    properties: &Value,
    valid_from: &Value,
    valid_until: &Value,
) -> Result<String> {
    use sha1::{Digest, Sha1};
    let payload = json!({"source":source,"target":target,"type":kind,"weight":weight,"valid_from":valid_from,"valid_until":valid_until,"metadata":properties});
    let namespace = [
        0x6b, 0xa7, 0xb8, 0x11, 0x9d, 0xad, 0x11, 0xd1, 0x80, 0xb4, 0x00, 0xc0, 0x4f, 0xd4, 0x30,
        0xc8,
    ];
    let mut hash = Sha1::new();
    hash.update(namespace);
    hash.update(ascii_json(&payload)?);
    let mut digest = hash.finalize();
    digest[6] = (digest[6] & 15) | 0x50;
    digest[8] = (digest[8] & 63) | 0x80;
    let hex = digest[..16]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    ))
}
fn add_edge(
    data: &mut Records,
    source: &str,
    target: &str,
    kind: &str,
    properties: Value,
) -> Result<bool> {
    let id = edge_identity(source, target, kind, 1.0, &properties)?;
    validate_skos_hierarchy(
        &data.edges,
        &json!({"source":source,"target":target,"type":kind}),
    )?;
    if data.edges.iter().any(|e| e["id"] == id) {
        return Ok(false);
    }
    for endpoint in [source, target] {
        if !data.nodes.iter().any(|n| n["id"] == endpoint) {
            add_node(data, endpoint, "entity", endpoint, json!({}))?;
        }
    }
    data.edges.push(json!({"id":id,"familyId":id,"source":source,"target":target,"type":kind,"weight":1.0,"properties":properties}));
    Ok(true)
}
/// Original SKOS child/parent normalization and touched-component cycle rule.
/// The temporary adjacency is discarded after validation; DatasetSession owns state.
pub(crate) fn validate_skos_hierarchy(existing: &[Value], candidate: &Value) -> Result<()> {
    fn pair(edge: &Value) -> Option<(String, String)> {
        let kind = ["type", "edge_type", "relationship", "predicate", "relation"]
            .iter()
            .filter_map(|key| edge.get(*key).and_then(Value::as_str))
            .find(|kind| ["skos:broader", "skos:narrower"].contains(kind))?;
        let source = edge
            .get("source")
            .or_else(|| edge.get("source_id"))?
            .as_str()?
            .trim();
        let target = edge
            .get("target")
            .or_else(|| edge.get("target_id"))?
            .as_str()?
            .trim();
        if source.is_empty() || target.is_empty() {
            return None;
        }
        Some(if kind == "skos:broader" {
            (source.into(), target.into())
        } else {
            (target.into(), source.into())
        })
    }
    let Some((child, parent)) = pair(candidate) else {
        return Ok(());
    };
    let mut parents: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for edge in existing {
        if let Some((child, parent)) = pair(edge) {
            parents.entry(child).or_default().insert(parent);
        }
    }
    parents
        .entry(child.clone())
        .or_default()
        .insert(parent.clone());
    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    // An explicit DFS stack keeps valid deep hierarchies off the call stack.
    for root in [child, parent] {
        let mut stack = vec![(root, false)];
        while let Some((concept, exit)) = stack.pop() {
            if exit {
                visiting.remove(&concept);
                visited.insert(concept);
                continue;
            }
            if visited.contains(&concept) {
                continue;
            }
            ensure!(
                visiting.insert(concept.clone()),
                "SKOS hierarchy contains a cycle involving '{concept}'."
            );
            stack.push((concept.clone(), true));
            if let Some(next) = parents.get(&concept) {
                for parent in next.iter().rev() {
                    stack.push((parent.clone(), false));
                }
            }
        }
    }
    Ok(())
}
fn text<'a>(args: &'a Value, key: &str, default: &'a str) -> Result<&'a str> {
    args.get(key)
        .map(|v| {
            v.as_str()
                .context(format!("context_argument_not_string:{key}"))
        })
        .transpose()
        .map(|v| v.unwrap_or(default))
}
fn integer(args: &Value, key: &str, default: i64) -> Result<i64> {
    match args.get(key) {
        None => Ok(default),
        Some(Value::Bool(b)) => Ok(i64::from(*b)),
        Some(Value::Number(n)) => n
            .as_i64()
            .or_else(|| {
                n.as_f64()
                    .filter(|n| n.is_finite() && *n >= i64::MIN as f64 && *n < i64::MAX as f64)
                    .map(|n| n as i64)
            })
            .context("context_integer_invalid"),
        Some(Value::String(s)) => Ok(s.trim().parse()?),
        _ => bail!("context_integer_invalid"),
    }
}
fn slice<T>(items: &mut Vec<T>, limit: i64) {
    let end = if limit < 0 {
        items.len().saturating_sub(limit.unsigned_abs() as usize)
    } else {
        (limit as usize).min(items.len())
    };
    items.truncate(end);
}
fn row(node: &Value) -> Value {
    json!({"id":node["id"],"type":node["type"],"content":node.get("content").and_then(Value::as_str).unwrap_or_else(||node["id"].as_str().unwrap_or("")),"metadata":node.get("properties").cloned().or_else(||node.get("metadata").cloned()).unwrap_or(json!({}))})
}
fn decisions(data: &Records) -> Vec<Value> {
    let mut nodes = data
        .nodes
        .iter()
        .filter(|n| n["type"] == "decision")
        .collect::<Vec<_>>();
    nodes.sort_by_key(|n| n["id"].as_str());
    nodes.into_iter().map(row).collect()
}
fn outgoing(data: &Records, id: &str) -> usize {
    data.edges.iter().filter(|e| e["source"] == id).count()
}
fn search(data: &Records, scenario: &str, limit: i64, now: f64) -> Vec<Value> {
    let words = scenario
        .to_lowercase()
        .split_whitespace()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    let mut result = Vec::new();
    for node in &data.nodes {
        let Some(d) = node.get("decision_record") else {
            continue;
        };
        if d["valid_until"]
            .as_str()
            .and_then(parse_time)
            .is_some_and(|until| now > until)
        {
            continue;
        }
        let content = format!(
            "{} {} {}",
            d["scenario"].as_str().unwrap_or(""),
            d["reasoning"].as_str().unwrap_or(""),
            d["entities"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" ")
        );
        let other = content
            .to_lowercase()
            .split_whitespace()
            .map(str::to_owned)
            .collect::<BTreeSet<_>>();
        let union = words.union(&other).count();
        let cs = if union == 0 {
            0.0
        } else {
            words.intersection(&other).count() as f64 / union as f64
        };
        let connections = outgoing(data, node["id"].as_str().unwrap_or(""));
        let ss = data
            .nodes
            .iter()
            .filter(|n| n["id"] != node["id"] && n["type"] == node["type"])
            .map(|n| {
                let count = outgoing(data, n["id"].as_str().unwrap_or(""));
                1.0 - connections.abs_diff(count) as f64 / connections.max(count).max(1) as f64
            })
            .fold(0.0, f64::max);
        let similarity = 0.7 * cs + 0.3 * ss;
        if similarity >= 0.3 {
            result.push(json!({"decision":d,"similarity":similarity,"content_similarity":cs,"structural_similarity":ss}));
        }
    }
    result.sort_by(|a, b| {
        b["similarity"]
            .as_f64()
            .unwrap()
            .total_cmp(&a["similarity"].as_f64().unwrap())
    });
    slice(&mut result, limit);
    result
}
fn parse_time(s: &str) -> Option<f64> {
    let trimmed = s.trim();
    let year;
    if trimmed.len() == 4 && trimmed.bytes().all(|b| b.is_ascii_digit()) {
        year = format!("{trimmed}-01-01");
    } else {
        year = trimmed.into();
    }
    let s = year.as_str();
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|d| d.timestamp_micros() as f64 / 1e6)
        .ok()
        .or_else(|| {
            chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f")
                .map(|d| d.and_utc().timestamp_micros() as f64 / 1e6)
                .ok()
        })
        .or_else(|| {
            chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .ok()
                .and_then(|d| d.and_hms_opt(0, 0, 0))
                .map(|d| d.and_utc().timestamp() as f64)
        })
}
/// Original timezone-naive ISO observation representation, independent of build/adoption identity.
pub fn observation_time(epoch: f64) -> Result<String> {
    use chrono::Datelike;
    ensure!(epoch.is_finite(), "context_temporal_invalid");
    let micros = (epoch * 1e6).round();
    ensure!(
        micros >= i64::MIN as f64 && micros < i64::MAX as f64,
        "context_temporal_invalid"
    );
    let date = chrono::DateTime::from_timestamp_micros(micros as i64)
        .context("context_temporal_invalid")?;
    ensure!(
        (1..=9999).contains(&date.year()),
        "context_temporal_invalid"
    );
    Ok(date.naive_utc().format("%Y-%m-%dT%H:%M:%S%.f").to_string())
}
/// Original ContextGraph numeric causal timestamp uses the host local timezone.
/// The OS routine changes no process timezone; non-Unix hosts are explicit gaps.
#[cfg(unix)]
fn local_observation_time(epoch: f64) -> Result<String> {
    ensure!(epoch.is_finite(), "context_temporal_invalid");
    let micros = (epoch * 1e6).round();
    ensure!(
        micros >= i64::MIN as f64 && micros < i64::MAX as f64,
        "context_temporal_invalid"
    );
    let micros = micros as i64;
    let seconds = micros.div_euclid(1_000_000) as libc::time_t;
    let mut output = std::mem::MaybeUninit::<libc::tm>::uninit();
    // SAFETY: immutable valid time_t input and writable storage for one tm;
    // localtime_r initializes the full output on success and returns null otherwise.
    let initialized = unsafe { libc::localtime_r(&seconds, output.as_mut_ptr()) };
    ensure!(!initialized.is_null(), "context_local_time_invalid");
    // SAFETY: non-null localtime_r result confirms initialization above.
    let time = unsafe { output.assume_init() };
    let date = chrono::NaiveDate::from_ymd_opt(
        time.tm_year + 1900,
        (time.tm_mon + 1) as u32,
        time.tm_mday as u32,
    )
    .context("context_local_time_invalid")?;
    let datetime = date
        .and_hms_micro_opt(
            time.tm_hour as u32,
            time.tm_min as u32,
            time.tm_sec as u32,
            micros.rem_euclid(1_000_000) as u32,
        )
        .context("context_local_time_invalid")?;
    Ok(datetime.format("%Y-%m-%dT%H:%M:%S%.f").to_string())
}
#[cfg(not(unix))]
fn local_observation_time(_epoch: f64) -> Result<String> {
    bail!("context_local_timezone_uncovered")
}
fn normalize_temporal(value: &Value) -> Result<Value> {
    let epoch = match value {
        Value::Null => return Ok(Value::Null),
        Value::String(s) => parse_time(s).context("context_temporal_invalid")?,
        Value::Number(n) => n.as_f64().context("context_temporal_invalid")?,
        Value::Bool(b) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        _ => bail!("context_temporal_invalid"),
    };
    Ok(json!(observation_time(epoch)?))
}
/// Original UUID4 observation identifiers, filled by the OS CSPRNG.
/// Identifier generation confers neither authority nor truth on its record.
pub fn new_decision_id() -> Result<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|_| anyhow::anyhow!("context_event_entropy_unavailable"))?;
    bytes[6] = (bytes[6] & 15) | 0x40;
    bytes[8] = (bytes[8] & 63) | 0x80;
    let raw = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &raw[..8],
        &raw[8..12],
        &raw[12..16],
        &raw[16..20],
        &raw[20..]
    ))
}
/// Host-controlled event fields; they name an observation and confer no authority.
pub struct DecisionEvent<'a> {
    pub id: &'a str,
    pub timestamp: f64,
    pub recorded_at: &'a str,
}
fn record_decision(data: &mut Records, args: &Value, event: &DecisionEvent<'_>) -> Result<Value> {
    for key in ["category", "scenario", "reasoning", "outcome", "confidence"] {
        if args.get(key).is_none() {
            return Ok(json!({"error":format!("missing required field: {key}")}));
        }
    }
    let mut strings = BTreeMap::new();
    for (key, max) in [
        ("category", 100),
        ("scenario", 5000),
        ("reasoning", 10000),
        ("outcome", 1000),
    ] {
        let value = text(args, key, "")?.trim();
        ensure!(
            !value.is_empty() && value.chars().count() <= max,
            "context_decision_text_invalid:{key}"
        );
        strings.insert(key, value);
    }
    let confidence = match &args["confidence"] {
        Value::Bool(b) => f64::from(*b),
        Value::String(s) => s.trim().parse()?,
        v => v.as_f64().context("context_confidence_invalid")?,
    };
    ensure!(
        confidence.is_finite() && (0.0..=1.0).contains(&confidence),
        "context_confidence_invalid"
    );
    let entities = if let Some(raw) = args.get("entities").filter(|raw| !raw.is_null()) {
        raw.as_array()
            .context("context_entities_invalid")?
            .iter()
            .map(|v| {
                let text = v.as_str().context("context_entity_not_string")?.trim();
                ensure!(
                    !text.is_empty() && text.chars().count() <= 200,
                    "context_entity_too_long"
                );
                Ok(text.to_owned())
            })
            .collect::<Result<Vec<_>>>()?
    } else {
        Vec::new()
    };
    let entities = entities
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    let maker = match args.get("decision_maker") {
        Some(Value::Null) => "",
        None => "mcp_client",
        Some(v) => {
            let raw = v.as_str().context("context_maker_invalid")?.trim();
            ensure!(!raw.is_empty(), "context_maker_invalid");
            raw
        }
    };
    ensure!(maker.chars().count() <= 200, "context_maker_too_long");
    let mut validity = BTreeMap::new();
    for key in ["valid_from", "valid_until"] {
        validity.insert(
            key,
            normalize_temporal(args.get(key).unwrap_or(&Value::Null))?,
        );
    }
    ensure!(
        !event.id.trim().is_empty()
            && !data.nodes.iter().any(|n| n["id"] == event.id)
            && event.timestamp.is_finite(),
        "context_decision_event_invalid"
    );
    let properties = json!({"category":strings["category"],"scenario":strings["scenario"],"reasoning":strings["reasoning"],"outcome":strings["outcome"],"confidence":confidence,"timestamp":event.timestamp,"decision_maker":if maker.is_empty(){Value::Null}else{json!(maker)}});
    add_node(data, event.id, "decision", strings["scenario"], properties)?;
    let decision = json!({"id":event.id,"category":strings["category"],"scenario":strings["scenario"],"reasoning":strings["reasoning"],"outcome":strings["outcome"],"confidence":confidence,"entities":entities,"decision_maker":if maker.is_empty(){Value::Null}else{json!(maker)},"timestamp":event.timestamp,"recorded_at":event.recorded_at,"valid_from":validity["valid_from"],"valid_until":validity["valid_until"],"metadata":{}});
    let node = data.nodes.iter_mut().find(|n| n["id"] == event.id).unwrap();
    node["decision_record"] = decision;
    node["valid_from"] = validity["valid_from"].clone();
    node["valid_until"] = validity["valid_until"].clone();
    for entity in entities {
        if !data.nodes.iter().any(|n| n["id"] == entity) {
            add_node(data, &entity, "entity", &entity, json!({"name":entity}))?;
        }
        add_edge(
            data,
            event.id,
            &entity,
            "involves",
            json!({"confidence":confidence}),
        )?;
    }
    for (id, kind, name, relationship) in [
        (
            format!("category_{}", strings["category"]),
            "category",
            strings["category"],
            "belongs_to",
        ),
        (format!("maker_{maker}"), "decision_maker", maker, "made_by"),
    ] {
        if name.is_empty() {
            continue;
        }
        if !data.nodes.iter().any(|n| n["id"] == id) {
            add_node(data, &id, kind, &id, json!({"name":name}))?;
        }
        add_edge(data, event.id, &id, relationship, json!({}))?;
    }
    Ok(json!({"decision_id":event.id,"status":"recorded"}))
}
fn causal(data: &Records, id: &str, direction: &str, depth: i64) -> Result<Value> {
    ensure!(
        ["upstream", "downstream"].contains(&direction),
        "Direction must be 'upstream' or 'downstream'"
    );
    use std::collections::VecDeque;
    let mut queue = VecDeque::from([(id.to_owned(), 0i64)]);
    let mut visited = BTreeSet::new();
    let mut result = Vec::new();
    while let Some((current, level)) = queue.pop_front() {
        if level > depth || !visited.insert(current.clone()) {
            continue;
        }
        if current != id {
            if let Some(node) = data.nodes.iter().find(|n| {
                n["id"] == current
                    && n["type"]
                        .as_str()
                        .is_some_and(|s| s.eq_ignore_ascii_case("decision"))
            }) {
                let p = &node["properties"];
                let mut metadata = p.as_object().cloned().unwrap_or_default();
                for key in [
                    "category",
                    "scenario",
                    "reasoning",
                    "outcome",
                    "confidence",
                    "timestamp",
                    "decision_maker",
                    "reasoning_embedding",
                    "node2vec_embedding",
                ] {
                    metadata.remove(key);
                }
                metadata.insert("causal_distance".into(), json!(level));
                let timestamp = if let Some(epoch) = p["timestamp"].as_f64() {
                    Some(local_observation_time(epoch)?)
                } else {
                    p["timestamp"]
                        .as_str()
                        .map(|v| v.trim_end_matches('Z').to_owned())
                };
                result.push(json!({"decision_id":current,"category":p.get("category").cloned().unwrap_or(json!("")),"scenario":p.get("scenario").cloned().unwrap_or(node["content"].clone()),"reasoning":p.get("reasoning").cloned().unwrap_or(json!("")),"outcome":p.get("outcome").cloned().unwrap_or(json!("")),"confidence":p.get("confidence").cloned().unwrap_or(json!(0.0)),"timestamp":timestamp,"decision_maker":p.get("decision_maker").cloned().unwrap_or(json!("")),"reasoning_embedding":p.get("reasoning_embedding").cloned().unwrap_or(Value::Null),"node2vec_embedding":p.get("node2vec_embedding").cloned().unwrap_or(Value::Null),"valid_from":node.get("valid_from").cloned().unwrap_or(Value::Null),"valid_until":node.get("valid_until").cloned().unwrap_or(Value::Null),"metadata":metadata}));
            }
        }
        if level < depth {
            for edge in &data.edges {
                if !["CAUSED", "INFLUENCED", "PRECEDENT_FOR"]
                    .iter()
                    .any(|kind| edge["type"] == *kind)
                {
                    continue;
                }
                let (from, to) = if direction == "upstream" {
                    ("target", "source")
                } else {
                    ("source", "target")
                };
                if edge[from] == current {
                    queue.push_back((edge[to].as_str().unwrap_or("").to_owned(), level + 1));
                }
            }
        }
    }
    if direction == "upstream" {
        result.sort_by_key(|d| {
            std::cmp::Reverse(d["metadata"]["causal_distance"].as_i64().unwrap_or(0))
        });
    }
    Ok(json!({"chain":result}))
}
/// Native ContextGraph tools; controlled dispatch never approves an engineering decision.
pub fn execute_tool(
    session: &mut DatasetSession,
    name: &str,
    args: &Value,
    event: &DecisionEvent<'_>,
) -> Result<Value> {
    ensure!(args.is_object(), "context_arguments_not_object");
    let mut data = records(session)?;
    let mut changed = false;
    let result = match name {
        "add_entity" => {
            let id = text(args, "id", "")?;
            if id.is_empty() {
                json!({"error":"id is required"})
            } else {
                changed = add_node(
                    &mut data,
                    id,
                    text(args, "type", "Entity")?,
                    id,
                    json!({"label":args.get("label").cloned().unwrap_or(json!(id)),"metadata":args.get("metadata").cloned().unwrap_or(json!({}))}),
                )?;
                json!({"status":"added","id":id})
            }
        }
        "add_relationship" => {
            let source = text(args, "source", "")?;
            let target = text(args, "target", "")?;
            let kind = text(args, "type", "RELATED_TO")?;
            if source.is_empty() || target.is_empty() {
                json!({"error":"source and target are required"})
            } else {
                changed = add_edge(
                    &mut data,
                    source,
                    target,
                    kind,
                    json!({"metadata":args.get("metadata").cloned().unwrap_or(json!({}))}),
                )?;
                json!({"status":"added","source":source,"target":target,"type":kind})
            }
        }
        "record_decision" => {
            let value = record_decision(&mut data, args, event)?;
            changed = value.get("error").is_none();
            value
        }
        "query_decisions" => {
            let query = text(args, "query", "")?;
            let limit = integer(args, "limit", 10)?;
            if !query.is_empty() {
                json!({"decisions":search(&data,query,limit,event.timestamp)})
            } else {
                let mut list = decisions(&data);
                if args
                    .get("category")
                    .is_some_and(|v| v.as_str().is_some_and(|s| !s.is_empty()))
                {
                    list.retain(|row| row.get("category") == args.get("category"));
                }
                slice(&mut list, limit);
                json!({"decisions":list})
            }
        }
        "find_precedents" => {
            let scenario = text(args, "scenario", "")?;
            if scenario.is_empty() {
                json!({"error":"scenario is required"})
            } else {
                json!({"precedents":search(&data,scenario,integer(args,"max_results",5)?,event.timestamp)})
            }
        }
        "get_causal_chain" => {
            let id = text(args, "decision_id", "")?;
            if id.is_empty() {
                json!({"error":"decision_id is required"})
            } else {
                match causal(
                    &data,
                    id,
                    text(args, "direction", "downstream")?,
                    integer(args, "max_depth", 5)?,
                ) {
                    Ok(v) => v,
                    Err(e) => json!({"error":e.to_string(),"chain":[]}),
                }
            }
        }
        "get_graph_summary" => context_summary(session)?,
        "get_graph_analytics" => analytics(&data)?,
        "export_graph" => export(session, &data, text(args, "format", "json-ld")?)?,
        _ => bail!("context_tool_unknown"),
    };
    if changed {
        commit(session, &data)?;
    }
    Ok(result)
}

fn analytics(data: &Records) -> Result<Value> {
    // Corrected native profile: original canonical call fails at dict.nodes().
    // PageRank retains original 20 passes/0.85/l2 tolerance without dangling redistribution.
    ensure!(
        data.nodes.len() <= 256 && data.edges.len() <= 8192,
        "context_analytics_budget"
    );
    let count = data.nodes.len();
    if count == 0 {
        return Ok(json!({"error":"No nodes found matching the specified criteria"}));
    }
    let ids = data
        .nodes
        .iter()
        .map(|n| n["id"].as_str().unwrap_or(""))
        .collect::<Vec<_>>();
    let mut rank = vec![1.0 / count as f64; count];
    for _ in 0..20 {
        let mut next = vec![0.15 / count as f64; count];
        for (i, id) in ids.iter().enumerate() {
            let adjacent = data
                .edges
                .iter()
                .filter(|e| e["source"] == *id)
                .filter_map(|e| e["target"].as_str())
                .filter(|target| target != id)
                .collect::<BTreeSet<_>>();
            if adjacent.is_empty() {
                continue;
            }
            for target in &adjacent {
                if let Some(j) = ids.iter().position(|v| v == target) {
                    next[j] += 0.85 * rank[i] / adjacent.len() as f64;
                }
            }
        }
        let delta = rank
            .iter()
            .zip(&next)
            .map(|(a, b)| (a - b).powi(2))
            .sum::<f64>()
            .sqrt();
        rank = next;
        if delta < 1e-6 {
            break;
        }
    }
    let mut ranked = ids
        .iter()
        .zip(&rank)
        .map(|(id, value)| json!([id, value]))
        .collect::<Vec<_>>();
    ranked.sort_by(|a, b| b[1].as_f64().unwrap().total_cmp(&a[1].as_f64().unwrap()));
    ranked.truncate(10);
    let communities = greedy_communities(data);
    Ok(
        json!({"node_count":count,"edge_count":0,"top_nodes_by_pagerank":ranked,"community_count":communities,"_meta":{"profile":"semantica.native-context-analytics/1","original_canonical_error":"dict.nodes is not callable","pagerank":"original bounded power iteration; no dangling redistribution","community_algorithm":"greedy modularity on undirected simple graph"}}),
    )
}
fn greedy_communities(data: &Records) -> usize {
    let ids = data
        .nodes
        .iter()
        .map(|n| n["id"].as_str().unwrap_or(""))
        .collect::<Vec<_>>();
    let mut edges = BTreeSet::new();
    let mut degree = vec![0.0; ids.len()];
    for e in &data.edges {
        if let (Some(a), Some(b)) = (
            ids.iter().position(|v| e["source"] == *v),
            ids.iter().position(|v| e["target"] == *v),
        ) {
            let pair = (a.min(b), a.max(b));
            if edges.insert(pair) {
                degree[a] += 1.0;
                degree[b] += 1.0;
            }
        }
    }
    if edges.is_empty() {
        return ids.len();
    }
    let m = edges.len() as f64;
    let mut groups = (0..ids.len())
        .map(|i| BTreeSet::from([i]))
        .collect::<Vec<_>>();
    loop {
        let mut best = None;
        let mut gain = 0.0;
        for a in 0..groups.len() {
            for b in a + 1..groups.len() {
                let cross = edges
                    .iter()
                    .filter(|(x, y)| {
                        (groups[a].contains(x) && groups[b].contains(y))
                            || (groups[b].contains(x) && groups[a].contains(y))
                    })
                    .count() as f64;
                let da = groups[a].iter().map(|i| degree[*i]).sum::<f64>();
                let db = groups[b].iter().map(|i| degree[*i]).sum::<f64>();
                let delta = cross / m - da * db / (2.0 * m * m);
                if delta > gain {
                    gain = delta;
                    best = Some((a, b));
                }
            }
        }
        let Some((a, b)) = best else { break };
        let removed = groups.remove(b);
        groups[a].extend(removed);
    }
    groups.len()
}
fn export(session: &DatasetSession, data: &Records, format: &str) -> Result<Value> {
    let value = if ["turtle", "ttl", "nt", "xml", "json-ld"].contains(&format) {
        let kind = match format {
            "ttl" | "turtle" => RdfFormat::Turtle,
            "nt" => RdfFormat::NTriples,
            "xml" => RdfFormat::RdfXml,
            _ => RdfFormat::JsonLd {
                profile: Default::default(),
            },
        };
        json!(session.serialize(kind, Some(CONTEXT_GRAPH))?)
    } else {
        json!({"nodes":data.nodes,"edges":data.edges})
    };
    Ok(
        json!({"format":format,"data":value,"_meta":{"profile":"semantica.native-context-export/1","serialization_scope":"same context record named graph","original_canonical_export":"ContextGraph object shape/file_path mismatch retained in original oracle"}}),
    )
}

/// Original ContextGraph.stats finite initialized record view, on the sole DatasetSession.
/// Unrelated RDF and an uninitialized empty Dataset are not a ready GraphSession.
pub fn explorer_graph_stats(session: &DatasetSession) -> Result<Value> {
    ensure!(
        session.dataset().iter().any(
            |q| matches!(q.graph_name, GraphNameRef::NamedNode(n) if n.as_str()==CONTEXT_GRAPH)
        ),
        "context_projection_unavailable"
    );
    let data = records(session)?;
    let mut node_types = BTreeMap::<String, u64>::new();
    let mut edge_types = BTreeMap::<String, u64>::new();
    for node in &data.nodes {
        let kind = match node.get("type") {
            None => "entity",
            Some(v) => v.as_str().context("context_node_type_invalid")?,
        };
        *node_types.entry(kind.to_owned()).or_default() += 1;
    }
    for edge in &data.edges {
        let kind = match edge.get("type") {
            None => "related_to",
            Some(v) => v.as_str().context("context_edge_type_invalid")?,
        };
        *edge_types.entry(kind.to_owned()).or_default() += 1;
    }
    let n = data.nodes.len();
    let density = if n < 2 {
        0.0
    } else {
        data.edges.len() as f64 / (n * (n - 1)) as f64
    };
    Ok(
        json!({"node_count":n,"edge_count":data.edges.len(),"node_types":node_types,"edge_types":edge_types,"density":density}),
    )
}
