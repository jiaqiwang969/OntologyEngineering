//! Original GraphSession mutation compatibility inside the sole DatasetSession.
//! A returned original exception is data: it may accompany committed graph changes.
//! Host validation, resource limits and deadlines remain native atomic refusals.
use crate::{
    context::{self, Records},
    dataset::DatasetSession,
    explorer_search_index::GraphSearchIndex,
};
use anyhow::{Context, Result, bail, ensure};
use oxigraph::model::{Dataset, GraphNameRef, Literal, NamedNode, Quad, TermRef};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};
const GRAPH: &str = "urn:semantica:explorer:mutation-state/1";
const SUBJECT: &str = "urn:semantica:explorer:mutation-state";
const PREDICATE: &str = "urn:semantica:explorer:mutation-value";
#[derive(Serialize, Deserialize)]
struct State {
    #[serde(skip)]
    extended: bool,
    schema: String,
    context_sha256: String,
    bridge: bool,
    graph_revision: u64,
    node_type_index: BTreeMap<String, BTreeSet<String>>,
    #[serde(deserialize_with = "GraphSearchIndex::deserialize_stored")]
    index: GraphSearchIndex,
}
fn timely(d: Instant) -> Result<()> {
    ensure!(Instant::now() < d, "explorer_mutation_deadline_exceeded");
    Ok(())
}
fn digest(data: &Records) -> Result<String> {
    Ok(crate::sha256(serde_json::to_vec(
        &json!({"nodes":data.nodes,"edges":data.edges}),
    )?))
}
fn stored(session: &DatasetSession) -> Result<Option<State>> {
    let mut found = None;
    for q in session.dataset().iter() {
        if matches!(q.graph_name,GraphNameRef::NamedNode(n)if n.as_str()==GRAPH) {
            ensure!(
                q.subject.to_string() == format!("<{SUBJECT}>")
                    && q.predicate.as_str() == PREDICATE
                    && found.is_none(),
                "explorer_mutation_state_shape"
            );
            let TermRef::Literal(l) = q.object else {
                bail!("explorer_mutation_state_literal")
            };
            let state: State = serde_json::from_str(l.value())?;
            ensure!(
                state.schema == "semantica.explorer-mutation-state/1"
                    && state.context_sha256 == digest(&context::records(session)?)?,
                "explorer_mutation_state_stale"
            );
            found = Some(state);
        }
    }
    Ok(found)
}
pub(crate) fn stored_index(session: &DatasetSession) -> Result<Option<GraphSearchIndex>> {
    Ok(stored(session)?.map(|s| s.index))
}
fn load(session: &mut DatasetSession, bridge: bool, d: Instant) -> Result<State> {
    if let Some(mut state) = stored(session)? {
        ensure!(
            state.bridge == bridge,
            "explorer_mutation_bridge_mode_changed"
        );
        state.index = session.explorer_mutation_index(d)?;
        return Ok(state);
    }
    let records = context::records(session)?;
    let mut types: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for n in &records.nodes {
        types
            .entry(string(&n["type"])?)
            .or_default()
            .insert(string(&n["id"])?);
    }
    Ok(State {
        extended: false,
        schema: "semantica.explorer-mutation-state/1".into(),
        context_sha256: digest(&records)?,
        bridge,
        graph_revision: 0,
        node_type_index: types,
        index: session.explorer_mutation_index(d)?,
    })
}
fn save(session: &mut DatasetSession, state: &State) -> Result<()> {
    state.index.validate_persistable()?;
    let encoded = serde_json::to_string(state)?;
    ensure!(
        encoded.len() <= session.dataset_byte_limit(),
        "explorer_mutation_state_budget"
    );
    let mut graph = Dataset::new();
    graph.insert(&Quad::new(
        NamedNode::new(SUBJECT)?,
        NamedNode::new(PREDICATE)?,
        Literal::new_simple_literal(encoded),
        NamedNode::new(GRAPH)?,
    ));
    session.replace_named_graph_owned(GRAPH, graph)?;
    Ok(())
}
fn truth(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(m) => !m.is_empty(),
        Value::Number(_) => !semantica_json::python_equal(v, &json!(0)).unwrap_or(false),
    }
}
fn string(v: &Value) -> Result<String> {
    Ok(semantica_json::python_str(v)?)
}
fn pick(values: impl IntoIterator<Item = Value>) -> Value {
    values
        .into_iter()
        .find(|v| {
            !v.is_null()
                && !v
                    .as_str()
                    .is_some_and(|s| semantica_json::python_strip(s).is_empty())
        })
        .unwrap_or(Value::Null)
}
fn first(v: &Value, keys: &[&str]) -> Value {
    pick(
        keys.iter()
            .map(|k| v.get(*k).cloned().unwrap_or(Value::Null)),
    )
}
fn props(v: &Value) -> Value {
    let mut p = json!({});
    for k in ["metadata", "properties"] {
        if let Some(m) = v[k].as_object() {
            for (k, v) in m {
                p[k] = v.clone();
            }
        }
    }
    p
}
fn field_error(e: &anyhow::Error) -> Option<&'static str> {
    match e.to_string().as_str() {
        "explorer_search_tags_not_iterable" => Some("TypeError"),
        "explorer_search_integer_float_overflow" | "explorer_mutation_weight_overflow" => {
            Some("OverflowError")
        }
        s if s.starts_with("SKOS hierarchy contains a cycle") => Some("ValueError"),
        _ => None,
    }
}
fn revision(state: &mut State) -> Result<()> {
    state.graph_revision = state
        .graph_revision
        .checked_add(1)
        .context("explorer_mutation_revision_overflow")?;
    Ok(())
}
fn callback_node(
    state: &mut State,
    node: &Value,
    d: Instant,
    warnings: &mut Vec<Value>,
) -> Result<()> {
    revision(state)?;
    if let Err(e) = state
        .index
        .upsert_original_partial(&crate::explorer_read::node(node)?, d)
    {
        if let Some(kind) = field_error(&e) {
            warnings.push(json!({"error_type":kind,"code":e.to_string()}));
        } else {
            return Err(e);
        }
    }
    Ok(())
}
fn put_node(
    data: &mut Records,
    state: &mut State,
    node: Value,
    d: Instant,
    warnings: &mut Vec<Value>,
) -> Result<bool> {
    ensure!(
        state.extended
            || (node["properties"]["valid_from"].is_null()
                && node["properties"]["valid_until"].is_null()),
        "explorer_mutation_temporal_node_pending"
    );
    timely(d)?;
    let id = node["id"]
        .as_str()
        .context("explorer_mutation_node_id_string")?;
    if semantica_json::python_strip(id).is_empty() {
        return Ok(false);
    }
    ensure!(
        if state.extended {
            id.chars().count() <= 512
        } else {
            id.len() <= 512
        },
        "context_node_identity_invalid"
    );
    let kind = node["type"].as_str().context("context_node_type_invalid")?;
    ensure!(
        !kind.is_empty() && kind.len() <= 512,
        "context_node_type_invalid"
    );
    state
        .node_type_index
        .entry(kind.into())
        .or_default()
        .insert(id.into());
    if let Some(old) = data.nodes.iter_mut().find(|n| n["id"] == id) {
        *old = node.clone();
    } else {
        data.nodes.push(node.clone());
    }
    if state.bridge {
        callback_node(state, &node, d, warnings)?;
    }
    Ok(true)
}
fn node(id: Value, kind: Value, content: Value, mut properties: Value) -> Value {
    let from = properties
        .as_object_mut()
        .unwrap()
        .remove("valid_from")
        .unwrap_or(Value::Null);
    let until = properties
        .as_object_mut()
        .unwrap()
        .remove("valid_until")
        .unwrap_or(Value::Null);
    properties["content"] = if truth(&content) { content } else { id.clone() };
    if !from.is_null() {
        properties["valid_from"] = from;
    }
    if !until.is_null() {
        properties["valid_until"] = until;
    }
    json!({"id":id,"type":kind,"properties":properties})
}
fn batch_nodes(
    data: &mut Records,
    state: &mut State,
    values: &[Value],
    d: Instant,
    warnings: &mut Vec<Value>,
) -> Result<usize> {
    let mut count = 0;
    for v in values {
        timely(d)?;
        if !v.is_object() {
            continue;
        }
        let raw_id = first(v, &["id", "node_id", "_id", "uri", "key"]);
        if raw_id.is_null() {
            continue;
        }
        let id = semantica_json::python_strip(&string(&raw_id)?).to_owned();
        if id.is_empty() {
            continue;
        }
        let mut p = props(v);
        let kind = pick([
            first(v, &["type", "node_type", "category", ":LABEL"]),
            p["type"].clone(),
            json!("entity"),
        ]);
        let content = pick([
            first(
                v,
                &["content", "text", "label", "name", "title", "pref_label"],
            ),
            first(
                &p,
                &["content", "text", "label", "name", "title", "pref_label"],
            ),
            json!(id),
        ]);
        for k in ["valid_from", "valid_until"] {
            let value = pick([v[k].clone(), p[k].clone()]);
            if !value.is_null() {
                p[k] = value;
            }
        }
        let kind = string(&if truth(&kind) { kind } else { json!("entity") })?;
        let content = string(&if truth(&content) { content } else { json!(id) })?;
        if put_node(
            data,
            state,
            node(json!(id), json!(kind), json!(content), p),
            d,
            warnings,
        )? {
            count += 1;
        }
    }
    Ok(count)
}
fn prevalidate(data: &Records, edges: &[Value]) -> Result<()> {
    let mut combined = data.edges.clone();
    for e in edges {
        context::validate_skos_hierarchy(&combined, e)?;
        combined.push(e.clone());
    }
    Ok(())
}
fn edge(
    data: &mut Records,
    state: &mut State,
    source: String,
    target: String,
    kind: String,
    weight: &Value,
    mut p: Value,
    d: Instant,
    warnings: &mut Vec<Value>,
) -> Result<bool> {
    timely(d)?;
    let weight = if weight.is_null() || weight == "" {
        1.0
    } else {
        match semantica_json::python_builtin_float(weight) {
            Ok(n) => n,
            Err(semantica_json::PythonFloatError::IntegerOverflow) => {
                bail!("explorer_mutation_weight_overflow")
            }
            Err(_) => 1.0,
        }
    };
    ensure!(
        weight.is_finite(),
        "explorer_mutation_nonfinite_weight_unsupported"
    );
    let m = p
        .as_object_mut()
        .context("explorer_mutation_properties_object")?;
    let id = m.remove("id");
    let other = m.remove("edge_id");
    let explicit = pick([id.unwrap_or(Value::Null), other.unwrap_or(Value::Null)]);
    let family = m.remove("familyId");
    let other = m.remove("family_id");
    let family = pick([family.unwrap_or(Value::Null), other.unwrap_or(Value::Null)]);
    let from = m.remove("valid_from").unwrap_or(Value::Null);
    let until = m.remove("valid_until").unwrap_or(Value::Null);
    ensure!(
        state.extended || (from.is_null() && until.is_null()),
        "explorer_mutation_temporal_edge_pending"
    );
    let computed =
        context::edge_identity_with_validity(&source, &target, &kind, weight, &p, &from, &until)?;
    let id = if explicit.is_null() {
        computed
    } else {
        string(&explicit)?
    };
    let family = if family.is_null() {
        id.clone()
    } else {
        string(&family)?
    };
    if data.edges.iter().any(|e| e["id"] == id) {
        return Ok(false);
    }
    for endpoint in [&source, &target] {
        if !data.nodes.iter().any(|n| n["id"] == *endpoint) {
            put_node(
                data,
                state,
                node(json!(endpoint), json!("entity"), json!(endpoint), json!({})),
                d,
                warnings,
            )?;
        }
    }
    let mut record = json!({"id":id,"familyId":family,"source":source,"target":target,"type":kind,"weight":weight,"properties":p});
    if !from.is_null() {
        record["valid_from"] = from;
    }
    if !until.is_null() {
        record["valid_until"] = until;
    }
    data.edges.push(record);
    if state.bridge {
        revision(state)?;
    }
    Ok(true)
}
fn batch_edges(
    data: &mut Records,
    state: &mut State,
    values: &[Value],
    d: Instant,
    warnings: &mut Vec<Value>,
) -> Result<usize> {
    let mut count = 0;
    for v in values {
        timely(d)?;
        if !v.is_object() {
            continue;
        }
        let s = first(
            v,
            &[
                "source_id",
                "source",
                "start",
                "start_id",
                "from",
                "src",
                "START_ID",
                ":START_ID",
            ],
        );
        let t = first(
            v,
            &[
                "target_id",
                "target",
                "end",
                "end_id",
                "to",
                "dst",
                "END_ID",
                ":END_ID",
            ],
        );
        if s.is_null() || t.is_null() {
            continue;
        }
        let s = semantica_json::python_strip(&string(&s)?).to_owned();
        let t = semantica_json::python_strip(&string(&t)?).to_owned();
        if s.is_empty() || t.is_empty() {
            continue;
        }
        let mut p = props(v);
        let kind = pick([
            first(
                v,
                &[
                    "type",
                    "edge_type",
                    "relationship",
                    "predicate",
                    "relation",
                    ":TYPE",
                ],
            ),
            p["type"].clone(),
            json!("related_to"),
        ]);
        let weight = pick([v["weight"].clone(), p["weight"].clone()]);
        for (key, aliases) in [
            ("id", vec!["id", "edge_id"]),
            ("familyId", vec!["familyId", "family_id"]),
        ] {
            let value = pick([first(v, &aliases), first(&p, &aliases)]);
            for alias in &aliases {
                p.as_object_mut().unwrap().remove(*alias);
            }
            if !value.is_null() {
                p[key] = value;
            }
        }
        for k in ["valid_from", "valid_until"] {
            let value = pick([v[k].clone(), p[k].clone()]);
            if !value.is_null() {
                p[k] = value;
            }
        }
        if edge(
            data,
            state,
            s,
            t,
            string(&if truth(&kind) {
                kind
            } else {
                json!("related_to")
            })?,
            &weight,
            p,
            d,
            warnings,
        )? {
            count += 1;
        }
    }
    Ok(count)
}
fn apply(
    data: &mut Records,
    state: &mut State,
    call: &Value,
    d: Instant,
    warnings: &mut Vec<Value>,
) -> Result<Value> {
    let args = call["args"]
        .as_array()
        .context("explorer_mutation_args_array")?;
    let kw = call.get("kwargs").cloned().unwrap_or(json!({}));
    ensure!(kw.is_object(), "explorer_mutation_kwargs_object");
    let method = call["method"]
        .as_str()
        .context("explorer_mutation_method")?;
    ensure!(
        match method {
            "add_node" => (2..=3).contains(&args.len()),
            "add_edge" => (2..=4).contains(&args.len()),
            "add_nodes" | "add_edges" => args.len() == 1 && kw.as_object().unwrap().is_empty(),
            "add_nodes_and_edges" => args.len() == 2 && kw.as_object().unwrap().is_empty(),
            _ => false,
        },
        "explorer_mutation_call_shape_unsupported"
    );
    let arg = |i: usize| args.get(i).cloned().unwrap_or(Value::Null);
    match call["method"]
        .as_str()
        .context("explorer_mutation_method")?
    {
        "add_node" => {
            let n = node(arg(0), arg(1), arg(2), kw);
            let added = put_node(data, state, n.clone(), d, warnings)?;
            if added && !state.bridge {
                revision(state)?;
                state
                    .index
                    .upsert_original_partial(&crate::explorer_read::node(&n)?, d)?;
            }
            Ok(json!(added))
        }
        "add_edge" => {
            let (s, t, k) = (
                string(&arg(0))?,
                string(&arg(1))?,
                if args.len() > 2 {
                    string(&arg(2))?
                } else {
                    "related_to".into()
                },
            );
            prevalidate(data, &[json!({"source":s,"target":t,"type":k})])?;
            let added = edge(data, state, s, t, k, &arg(3), kw, d, warnings)?;
            if added && !state.bridge {
                revision(state)?;
            }
            Ok(json!(added))
        }
        method @ ("add_nodes" | "add_edges" | "add_nodes_and_edges") => {
            let nodes = if method == "add_edges" {
                vec![]
            } else {
                arg(0)
                    .as_array()
                    .context("explorer_mutation_nodes_array")?
                    .clone()
            };
            let edges = if method == "add_nodes" {
                vec![]
            } else {
                arg(if method == "add_edges" { 0 } else { 1 })
                    .as_array()
                    .context("explorer_mutation_edges_array")?
                    .clone()
            };
            prevalidate(data, &edges)?;
            let n = batch_nodes(data, state, &nodes, d, warnings)?;
            let e = batch_edges(data, state, &edges, d, warnings)?;
            if (n > 0 || e > 0) && !state.bridge {
                revision(state)?;
                let nodes = data
                    .nodes
                    .iter()
                    .map(crate::explorer_read::node)
                    .collect::<Result<Vec<_>>>()?;
                state.index.rebuild_original_partial(&nodes, d)?;
            }
            Ok(match method {
                "add_nodes" => json!(n),
                "add_edges" => json!(e),
                _ => json!([n, e]),
            })
        }
        _ => bail!("explorer_mutation_method_unsupported"),
    }
}
/// Host-only finite JSON compatibility operation. Native failure never publishes.
/// An original field/cycle exception is returned as an observation after publication.
pub fn execute(
    session: &mut DatasetSession,
    input: &semantica_contracts::SemanticExplorerMutationInput,
    d: Instant,
) -> Result<semantica_contracts::SemanticExplorerMutationResult> {
    ensure!(
        [
            "semantica.explorer-mutation-input/1",
            "semantica.explorer-mutation-input/2"
        ]
        .contains(&input.schema.as_str()),
        "explorer_mutation_profile_mismatch"
    );
    let bridge = input.bridge;
    let call = &input.call;
    timely(d)?;
    ensure!(
        serde_json::to_vec(call)?.len() <= session.dataset_byte_limit(),
        "explorer_mutation_input_budget"
    );
    let mut state = load(session, bridge, d)?;
    state.extended = input.schema == "semantica.explorer-mutation-input/2";
    let mut data = context::records(session)?;
    let before = digest(&data)?;
    let mut warnings = vec![];
    let result = match apply(&mut data, &mut state, call, d, &mut warnings) {
        Ok(value) => json!({"status":"returned","value":value}),
        Err(error) => {
            if let Some(kind) = field_error(&error) {
                json!({"status":"raised","error_type":kind,"code":error.to_string()})
            } else {
                return Err(error);
            }
        }
    };
    timely(d)?;
    let after = digest(&data)?;
    state.context_sha256 = after.clone();
    let mut staged = session.clone();
    if before != after {
        context::commit_original(&mut staged, &data)?;
    }
    save(&mut staged, &state)?;
    timely(d)?;
    *session = staged;
    Ok(semantica_contracts::SemanticExplorerMutationResult {
        schema: "semantica.explorer-mutation-result/1".into(),
        result,
        warnings: json!(warnings),
        graph_changed: before != after,
        graph_revision: state.graph_revision,
    })
}
/// Diagnostic view of original observable state; it grants no adoption authority.
pub fn observe(session: &DatasetSession) -> Result<Value> {
    let state = stored(session)?.context("explorer_mutation_state_missing")?;
    let data = context::records(session)?;
    let mut edges = data.edges;
    for edge in &mut edges {
        let e = edge.as_object_mut().unwrap();
        let s = e.remove("source").unwrap();
        let t = e.remove("target").unwrap();
        e.insert("source_id".into(), s);
        e.insert("target_id".into(), t);
    }
    let mut docs = serde_json::to_value(state.index)?["documents"].clone();
    for doc in docs.as_object_mut().unwrap().values_mut() {
        let doc = doc.as_object_mut().unwrap();
        for (from, to) in [
            ("id", "node_id"),
            ("exact", "exact_terms"),
            ("primary", "primary_text"),
            ("secondary", "secondary_text"),
        ] {
            let v = doc.remove(from).unwrap();
            doc.insert(to.into(), v);
        }
    }
    Ok(
        json!({"nodes":data.nodes,"edges":edges,"graph_revision":state.graph_revision,"node_type_index":state.node_type_index,"index_documents":docs}),
    )
}

pub(crate) mod native;
