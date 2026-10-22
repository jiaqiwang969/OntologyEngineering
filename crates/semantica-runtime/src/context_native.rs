//! Typed record storage inside the EXISTING context named graph and DatasetSession.
//! This is the storage dependency of native import, not an implemented import,
//! graph mutation bridge, index, HTTP serializer, or engineering adoption gate.
use crate::{context::CONTEXT_GRAPH, dataset::DatasetSession, native_values};
use anyhow::{Context, Result, bail, ensure};
use oxigraph::model::{Dataset, GraphNameRef, Literal, NamedNode, Quad, TermRef};
use semantica_contracts::SemanticNativeValueEnvelope;
use semantica_json::native_value::{NativeLimits, NativeNode as N, NativeValue};
use std::{collections::BTreeSet, time::Instant};

const ROOT: &str = "urn:semantica:context:root";
const SCHEMA: &str = "urn:semantica:context:schema";
const RECORDS: &str = "urn:semantica:context:nativeRecords";
const PROFILE: &str = "semantica.context-record-projection/3";
const STATE_PROFILE: &str = "semantica.context-object-state/4";
fn timely(d: Instant) -> Result<()> {
    ensure!(Instant::now() < d, "context_native_deadline_exceeded");
    Ok(())
}
fn text(v: &NativeValue, id: usize) -> Result<&[u32]> {
    match &v.nodes()[id] {
        N::Text(s) => Ok(s),
        _ => bail!("context_native_text_required"),
    }
}
fn object(v: &NativeValue, id: usize) -> Result<&[(usize, usize)]> {
    match &v.nodes()[id] {
        N::Dict(p) => Ok(p),
        _ => bail!("context_native_object_required"),
    }
}
fn field(v: &NativeValue, id: usize, name: &str) -> Result<usize> {
    object(v, id)?
        .iter()
        .find_map(|&(k, val)| {
            text(v, k)
                .ok()
                .filter(|s| s.iter().copied().eq(name.bytes().map(u32::from)))
                .map(|_| val)
        })
        .context("context_native_field_missing")
}
fn fields(v: &NativeValue, id: usize) -> Result<()> {
    let mut keys = BTreeSet::new();
    for &(k, _) in object(v, id)? {
        ensure!(keys.insert(text(v, k)?), "context_native_duplicate_field")
    }
    Ok(())
}
fn list(v: &NativeValue, id: usize) -> Result<&[usize]> {
    match &v.nodes()[id] {
        N::List(ids) => Ok(ids),
        _ => bail!("context_native_list_required"),
    }
}
fn validate_records(v: &NativeValue, node_limit: usize, d: Instant) -> Result<()> {
    timely(d)?;
    fields(v, v.root())?;
    ensure!(object(v, v.root())?.len() == 2, "context_native_root_shape");
    let nodes = list(v, field(v, v.root(), "nodes")?)?;
    let edges = list(v, field(v, v.root(), "edges")?)?;
    ensure!(
        nodes.len() <= node_limit && edges.len() <= 50_000,
        "context_record_count_budget"
    );
    let mut identities = BTreeSet::new();
    for &node in nodes {
        timely(d)?;
        fields(v, node)?;
        let id = text(v, field(v, node, "id")?)?;
        ensure!(identities.insert(id), "context_native_duplicate_node");
        field(v, node, "type")?;
        object(v, field(v, node, "properties")?)?;
    }
    let mut edge_ids = BTreeSet::new();
    for &edge in edges {
        timely(d)?;
        fields(v, edge)?;
        ensure!(
            edge_ids.insert(text(v, field(v, edge, "id")?)?),
            "context_native_duplicate_edge"
        );
        for name in ["source_id", "target_id"] {
            ensure!(
                identities.contains(text(v, field(v, edge, name)?)?),
                "context_native_endpoint_missing"
            );
        }
        field(v, edge, "type")?;
        object(v, field(v, edge, "properties")?)?;
        ensure!(
            matches!(v.nodes()[field(v, edge, "weight")?], N::FloatBits(_)),
            "context_native_weight_type"
        );
    }
    timely(d)
}
fn bounded(session: &DatasetSession, mut limits: NativeLimits) -> NativeLimits {
    limits.max_wire_bytes = limits.max_wire_bytes.min(session.dataset_byte_limit());
    limits
}

// The original to_dict and find_node views merge properties in different
// orders. Keep the actual fields once; never reconstruct them from /3.
fn validate_state(v: &NativeValue, node_limit: usize, d: Instant) -> Result<()> {
    timely(d)?;
    fields(v, v.root())?;
    ensure!(object(v, v.root())?.len() == 2, "context_native_root_shape");
    let nodes = list(v, field(v, v.root(), "nodes")?)?;
    let edges = list(v, field(v, v.root(), "edges")?)?;
    ensure!(
        nodes.len() <= node_limit && edges.len() <= 50_000,
        "context_record_count_budget"
    );
    let mut identities = BTreeSet::new();
    for &node in nodes {
        timely(d)?;
        fields(v, node)?;
        ensure!(
            object(v, node)?.len() == 7,
            "context_native_node_state_shape"
        );
        for name in [
            "id",
            "type",
            "content",
            "metadata",
            "properties",
            "valid_from",
            "valid_until",
        ] {
            field(v, node, name)?;
        }
        ensure!(
            identities.insert(text(v, field(v, node, "id")?)?),
            "context_native_duplicate_node"
        );
    }
    let mut edge_ids = BTreeSet::new();
    for &edge in edges {
        timely(d)?;
        fields(v, edge)?;
        ensure!(
            object(v, edge)?.len() == 9,
            "context_native_edge_state_shape"
        );
        for name in [
            "id",
            "family_id",
            "source_id",
            "target_id",
            "type",
            "weight",
            "metadata",
            "valid_from",
            "valid_until",
        ] {
            field(v, edge, name)?;
        }
        ensure!(
            edge_ids.insert(text(v, field(v, edge, "id")?)?),
            "context_native_duplicate_edge"
        );
        for name in ["source_id", "target_id"] {
            ensure!(
                identities.contains(text(v, field(v, edge, name)?)?),
                "context_native_endpoint_missing"
            );
        }
        text(v, field(v, edge, "family_id")?)?;
        text(v, field(v, edge, "type")?)?;
        ensure!(
            matches!(v.nodes()[field(v, edge, "weight")?], N::FloatBits(_)),
            "context_native_weight_type"
        );
    }
    timely(d)
}

fn publish_value(
    session: &mut DatasetSession,
    value: NativeValue,
    profile: &str,
    limits: NativeLimits,
    d: Instant,
) -> Result<()> {
    let wire = String::from_utf8(value.encode(limits)?)?;
    timely(d)?;
    let graph = NamedNode::new(CONTEXT_GRAPH)?;
    let root = NamedNode::new(ROOT)?;
    let mut replacement = Dataset::new();
    replacement.insert(&Quad::new(
        root.clone(),
        NamedNode::new(SCHEMA)?,
        Literal::new_simple_literal(profile),
        graph.clone(),
    ));
    replacement.insert(&Quad::new(
        root,
        NamedNode::new(RECORDS)?,
        Literal::new_simple_literal(wire),
        graph,
    ));
    let mut staged = session.clone();
    staged.replace_named_graph_owned(CONTEXT_GRAPH, replacement)?;
    timely(d)?;
    *session = staged;
    Ok(())
}

/// Trusted-host explicit profile selection. Retains raw ContextGraph records,
/// including source_id/target_id edges; does not reclassify imported wire as facts.
/// Source/governance checks belong to the existing host. Limits are supplied by it.
pub fn store_native_records(
    session: &mut DatasetSession,
    input: &SemanticNativeValueEnvelope,
    limits: NativeLimits,
    d: Instant,
) -> Result<()> {
    timely(d)?;
    let limits = bounded(session, limits);
    let value = native_values::decode(input, limits)?;
    validate_records(&value, session.context_node_limit(), d)?;
    // Use the original publisher and budgets. The private staging clone permits
    // a late deadline refusal without changing data, revision or derived caches.
    publish_value(session, value, PROFILE, limits, d)
}

/// Explicit /4 object fields. This is not a full ContextGraph export or an
/// implicit upgrade of /3: callers must supply the original separate fields.
pub fn store_native_context_state(
    session: &mut DatasetSession,
    input: &SemanticNativeValueEnvelope,
    limits: NativeLimits,
    d: Instant,
) -> Result<()> {
    timely(d)?;
    let limits = bounded(session, limits);
    let value = native_values::decode(input, limits)?;
    validate_state(&value, session.context_node_limit(), d)?;
    publish_value(session, value, STATE_PROFILE, limits, d)
}

/// Revalidates actual stored RDF and typed payload on every load; no sidecar
/// authority, cached proof, or ordinary-JSON projection can hide lost values.
fn read_value_profile(
    session: &DatasetSession,
    profile: &str,
    limits: NativeLimits,
    d: Instant,
) -> Result<(NativeValue, String)> {
    timely(d)?;
    let limits = bounded(session, limits);
    let (mut schema, mut records) = (None, None);
    for q in session.dataset().iter() {
        timely(d)?;
        if !matches!(q.graph_name,GraphNameRef::NamedNode(n)if n.as_str()==CONTEXT_GRAPH) {
            continue;
        }
        ensure!(
            q.subject.to_string() == format!("<{ROOT}>"),
            "context_native_subject"
        );
        let TermRef::Literal(l) = q.object else {
            bail!("context_native_literal")
        };
        ensure!(
            l.language().is_none()
                && l.datatype().as_str() == "http://www.w3.org/2001/XMLSchema#string",
            "context_native_literal_identity"
        );
        match q.predicate.as_str() {
            SCHEMA => {
                ensure!(
                    schema.replace(l.value()).is_none(),
                    "context_native_duplicate_schema"
                );
            }
            RECORDS => {
                ensure!(
                    records.replace(l.value()).is_none(),
                    "context_native_duplicate_payload"
                );
            }
            _ => bail!("context_native_unknown_property"),
        }
    }
    ensure!(schema == Some(profile), "context_native_profile_required");
    let raw = records.context("context_native_payload_missing")?;
    let value = NativeValue::decode(raw.as_bytes(), limits)?;
    if profile == STATE_PROFILE {
        validate_state(&value, session.context_node_limit(), d)?;
    } else {
        validate_records(&value, session.context_node_limit(), d)?;
    }
    let digest = crate::sha256(raw.as_bytes());
    timely(d)?;
    Ok((value, digest))
}

pub(crate) fn read_native_state_value(
    session: &DatasetSession,
    limits: NativeLimits,
    d: Instant,
) -> Result<(NativeValue, String)> {
    read_value_profile(session, STATE_PROFILE, limits, d)
}

pub fn read_native_records(
    session: &DatasetSession,
    limits: NativeLimits,
    d: Instant,
) -> Result<SemanticNativeValueEnvelope> {
    let (value, _) = read_value_profile(session, PROFILE, limits, d)?;
    let result = native_values::encode(&value, bounded(session, limits))?;
    timely(d)?;
    Ok(result)
}

pub fn read_native_context_state(
    session: &DatasetSession,
    limits: NativeLimits,
    d: Instant,
) -> Result<SemanticNativeValueEnvelope> {
    let (value, _) = read_native_state_value(session, limits, d)?;
    let result = native_values::encode(&value, bounded(session, limits))?;
    timely(d)?;
    Ok(result)
}
