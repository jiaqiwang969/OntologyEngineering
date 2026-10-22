//! Typed wire branch of the existing mutation state owner. No second graph.
use super::{GRAPH, PREDICATE, SUBJECT, timely};
use crate::{dataset::DatasetSession, explorer_search_index::GraphSearchIndex, native_values};
use anyhow::{Context, Result, bail, ensure};
use oxigraph::model::{Dataset, GraphNameRef, Literal, NamedNode, Quad, TermRef};
use semantica_contracts::{SemanticExplorerNativeState, SemanticNativeValueEnvelope};
use semantica_json::native_value::{NativeLimits, NativeNode as N, NativeValue};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};
pub(crate) fn field<'a>(value: &'a NativeValue, id: usize, key: &str) -> Result<usize> {
    let Some(N::Dict(pairs)) = value.nodes().get(id) else {
        bail!("explorer_native_record_mapping")
    };
    pairs
        .iter()
        .find_map(|(k, v)| match &value.nodes()[*k] {
            N::Text(s) if s.iter().copied().eq(key.chars().map(u32::from)) => Some(*v),
            _ => None,
        })
        .context("explorer_native_record_field")
}
pub(crate) fn node_ids(value: &NativeValue) -> Result<&[usize]> {
    match &value.nodes()[field(value, value.root(), "nodes")?] {
        N::List(ids) => Ok(ids),
        _ => bail!("explorer_native_record_nodes"),
    }
}
fn validate_types(
    input: &SemanticNativeValueEnvelope,
    limits: NativeLimits,
    d: Instant,
) -> Result<()> {
    let value = native_values::decode(input, limits)?;
    let N::Dict(types) = &value.nodes()[value.root()] else {
        bail!("explorer_native_type_index_shape")
    };
    let mut names = BTreeSet::new();
    for (k, v) in types {
        timely(d)?;
        let N::Text(name) = &value.nodes()[*k] else {
            bail!("explorer_native_type_index_key")
        };
        ensure!(names.insert(name), "explorer_native_type_index_duplicate");
        let N::List(ids) = &value.nodes()[*v] else {
            bail!("explorer_native_type_index_values")
        };
        let mut previous: Option<&Vec<u32>> = None;
        for id in ids {
            timely(d)?;
            let N::Text(id) = &value.nodes()[*id] else {
                bail!("explorer_native_type_index_id")
            };
            ensure!(
                previous.is_none_or(|p| p < id),
                "explorer_native_type_index_order"
            );
            previous = Some(id);
        }
    }
    timely(d)
}
pub(crate) fn initial_types(
    value: &NativeValue,
    limits: NativeLimits,
    d: Instant,
) -> Result<SemanticNativeValueEnvelope> {
    let unknown: Vec<u32> = "unknown".chars().map(u32::from).collect();
    let mut types: BTreeMap<&[u32], BTreeSet<&[u32]>> = BTreeMap::new();
    for id in node_ids(value)? {
        timely(d)?;
        let N::Text(identity) = &value.nodes()[field(value, *id, "id")?] else {
            bail!("explorer_native_node_id")
        };
        let kind = match &value.nodes()[field(value, *id, "type")?] {
            N::Text(s) => s.as_slice(),
            _ => &unknown,
        };
        types.entry(kind).or_default().insert(identity);
    }
    let (mut nodes, mut scalar, mut links) = (Vec::new(), 0usize, 0usize);
    let mut entries = Vec::new();
    fn text(nodes: &mut Vec<N>, scalar: &mut usize, s: &[u32], l: NativeLimits) -> Result<usize> {
        ensure!(
            nodes.len() < l.max_nodes,
            "explorer_native_type_node_budget"
        );
        *scalar = scalar
            .checked_add(
                s.len()
                    .checked_mul(4)
                    .context("explorer_native_type_scalar_budget")?,
            )
            .filter(|v| *v <= l.max_scalar_bytes)
            .context("explorer_native_type_scalar_budget")?;
        let id = nodes.len();
        nodes.push(N::Text(s.to_vec()));
        Ok(id)
    }
    for (kind, ids) in types {
        timely(d)?;
        links = links
            .checked_add(ids.len())
            .and_then(|n| n.checked_add(2))
            .filter(|n| *n <= limits.max_links)
            .context("explorer_native_type_link_budget")?;
        let key = text(&mut nodes, &mut scalar, kind, limits)?;
        let mut list = Vec::new();
        for id in ids {
            timely(d)?;
            list.push(text(&mut nodes, &mut scalar, id, limits)?);
        }
        ensure!(
            nodes.len() < limits.max_nodes,
            "explorer_native_type_node_budget"
        );
        let id = nodes.len();
        nodes.push(N::List(list));
        entries.push((key, id));
    }
    ensure!(
        nodes.len() < limits.max_nodes,
        "explorer_native_type_node_budget"
    );
    let root = nodes.len();
    nodes.push(N::Dict(entries));
    native_values::encode(&NativeValue::new(nodes, root, limits)?, limits)
}
pub(crate) fn load(
    session: &DatasetSession,
    digest: &str,
    limits: NativeLimits,
    d: Instant,
) -> Result<Option<(SemanticExplorerNativeState, GraphSearchIndex)>> {
    let mut result = None;
    for q in session.dataset().iter() {
        timely(d)?;
        if !matches!(q.graph_name,GraphNameRef::NamedNode(n)if n.as_str()==GRAPH) {
            continue;
        }
        ensure!(
            result.is_none()
                && q.subject.to_string() == format!("<{SUBJECT}>")
                && q.predicate.as_str() == PREDICATE,
            "explorer_native_state_shape"
        );
        let TermRef::Literal(l) = q.object else {
            bail!("explorer_native_state_literal")
        };
        ensure!(
            l.language().is_none()
                && l.datatype().as_str() == "http://www.w3.org/2001/XMLSchema#string",
            "explorer_native_state_literal"
        );
        ensure!(
            l.value().len() <= limits.max_wire_bytes,
            "explorer_native_state_budget"
        );
        let raw = semantica_json::strict_json(l.value().as_bytes())?;
        let fields = raw.as_object().context("explorer_native_state_shape")?;
        ensure!(
            fields.len() == 6
                && [
                    "schema",
                    "context_sha256",
                    "bridge",
                    "graph_revision",
                    "node_type_index",
                    "index"
                ]
                .iter()
                .all(|k| fields.contains_key(*k)),
            "explorer_native_state_shape"
        );
        NativeValue::from_envelope(&raw["node_type_index"], limits)?;
        NativeValue::from_envelope(&raw["index"], limits)?;
        let state: SemanticExplorerNativeState = serde_json::from_value(raw)?;
        ensure!(
            state.validate().is_ok() && state.context_sha256 == digest,
            "explorer_native_state_stale"
        );
        validate_types(&state.node_type_index, limits, d)?;
        let index = GraphSearchIndex::restore_native(&state.index, limits, d)?;
        result = Some((state, index));
    }
    timely(d)?;
    Ok(result)
}
pub(crate) fn save(
    session: &mut DatasetSession,
    state: &SemanticExplorerNativeState,
    limits: NativeLimits,
    d: Instant,
) -> Result<()> {
    timely(d)?;
    ensure!(state.validate().is_ok(), "explorer_native_state_contract");
    validate_types(&state.node_type_index, limits, d)?;
    let encoded = serde_json::to_string(state)?;
    ensure!(
        encoded.len() <= session.dataset_byte_limit().min(limits.max_wire_bytes),
        "explorer_native_state_budget"
    );
    let mut graph = Dataset::new();
    graph.insert(&Quad::new(
        NamedNode::new(SUBJECT)?,
        NamedNode::new(PREDICATE)?,
        Literal::new_simple_literal(encoded),
        NamedNode::new(GRAPH)?,
    ));
    session.replace_named_graph_owned(GRAPH, graph)?;
    timely(d)
}
