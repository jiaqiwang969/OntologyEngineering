//! Reversible transport of the full checked context. This is not summarization,
//! relevance selection, semantic execution or permission to adopt a conclusion.
use anyhow::{Context, Result, ensure};
use oe_contracts::HarnessContextPolicy;
use oe_core::{
    json::{canonical_json, ordered_json, strict_json},
    sha256,
};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub const POLICY_SCHEMA: &str = "oev2.harness-context-policy/1";
pub const EXPANDED_POLICY_SCHEMA: &str = "oev2.harness-context-policy/2";
pub const INDEXED_POLICY_SCHEMA: &str = "oev2.harness-context-policy/3";
pub const EXPANDED_MAX_CONTEXT_BYTES: usize = 256_000;
pub const VIEW_SCHEMA: &str = "oev2.harness-shared-context/1";
pub const INDEXED_VIEW_SCHEMA: &str = "oev2.harness-shared-context/2";
const REF: &str = "$oev2_ref";
const LITERAL: &str = "$oev2_literal";
const ROWS: &str = "$oev2_rows";
const MAX_DEPTH: usize = 96;
const MAX_NODES: usize = 200_000;
const ENCODER_VERSION: &str = "oev2.harness-sharing/2";
const INDEXED_ENCODER_VERSION: &str = "oev2.harness-sharing/3";

pub fn policy(value: Option<&Value>) -> Result<HarnessContextPolicy> {
    let p = match value {
        Some(v) => {
            let o = v.as_object().context("invalid_harness_context_policy")?;
            ensure!(
                o.len() == 3
                    && ["schema", "mode", "byte_budget"]
                        .iter()
                        .all(|k| o.contains_key(*k)),
                "invalid_harness_context_policy"
            );
            serde_json::from_value(v.clone()).context("invalid_harness_context_policy")?
        }
        None => HarnessContextPolicy {
            schema: POLICY_SCHEMA.into(),
            mode: "full".into(),
            byte_budget: 96_000usize as u64,
        },
    };
    byte_limit(&p)?;
    Ok(p)
}

/// A versioned local UTF-8 request guard, not a provider token allowance.
/// Legacy callers retain their original upper bound and default. A larger
/// request requires an explicit /2 policy; neither failure nor an account pool
/// can select it automatically.
pub(crate) fn byte_limit(p: &HarnessContextPolicy) -> Result<usize> {
    let maximum = match p.schema.as_str() {
        POLICY_SCHEMA => 128_000usize,
        EXPANDED_POLICY_SCHEMA => EXPANDED_MAX_CONTEXT_BYTES,
        INDEXED_POLICY_SCHEMA => EXPANDED_MAX_CONTEXT_BYTES,
        _ => anyhow::bail!("invalid_harness_context_policy"),
    };
    ensure!(
        (if p.schema == INDEXED_POLICY_SCHEMA {
            p.mode == "lossless_indexed_text"
        } else {
            ["full", "lossless_shared", "lossless_shared_text"].contains(&p.mode.as_str())
        }) && (28_000..=maximum as u64).contains(&p.byte_budget),
        "invalid_harness_context_policy"
    );
    Ok(maximum)
}

/// An embedded condition string retains its original UTF-8 digest. All parsed
/// values are transported, with lexical whitespace accounted for separately.
/// Unknown text stays text; no arbitrary field is parsed as a contract.
fn normalized(full: &Value) -> Result<Value> {
    let mut value = full.clone();
    fn conditions(state: &mut Value) -> Result<()> {
        if let Some(a) = state.get_mut("conditions").and_then(Value::as_array_mut) {
            for item in a {
                if let Some(raw) = item.as_str()
                    && let Ok(parsed) = strict_json(raw.as_bytes())
                    && matches!(parsed, Value::Object(_) | Value::Array(_))
                {
                    *item = json!({"encoding":"embedded-condition-json/1",
                        "original_utf8_sha256":sha256(raw.as_bytes()),
                        "lexical_scope":"original text archived; all JSON values retained here",
                        "value":parsed});
                }
            }
        }
        Ok(())
    }
    conditions(&mut value["project_state"])?;
    if !value["previous"].is_null() {
        conditions(&mut value["previous"]["state"])?;
    }
    if let Some(change) = value["changes"].get_mut("conditions") {
        for side in ["before", "after"] {
            if let Some(original) = change.get(side).cloned() {
                let mut state = json!({"conditions":original});
                conditions(&mut state)?;
                change[side] = state["conditions"].clone();
            }
        }
    }
    Ok(value)
}

fn reserved(o: &Map<String, Value>) -> bool {
    o.len() == 1 && [REF, LITERAL, ROWS].iter().any(|k| o.contains_key(*k))
}
fn digest(v: &Value) -> Result<String> {
    Ok(sha256(canonical_json(v)?))
}
fn count(
    v: &Value,
    counts: &mut BTreeMap<String, usize>,
    nodes: &mut usize,
    depth: usize,
) -> Result<()> {
    *nodes += 1;
    ensure!(
        *nodes <= MAX_NODES && depth <= MAX_DEPTH,
        "harness_context_structure_budget_exceeded"
    );
    if canonical_json(v)?.len() >= 160 {
        *counts.entry(digest(v)?).or_default() += 1;
    }
    match v {
        Value::Array(a) => {
            for x in a {
                count(x, counts, nodes, depth + 1)?;
            }
        }
        Value::Object(o) => {
            for x in o.values() {
                count(x, counts, nodes, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}
struct Encoder {
    counts: BTreeMap<String, usize>,
    dictionary: BTreeMap<String, Value>,
    total_transport_cost: bool,
}
impl Encoder {
    // Short references alone are not a saving: include every dictionary entry
    // reachable from this alternative, once, including its JSON key/framing.
    fn transport_bytes(&self, value: &Value) -> Result<usize> {
        if !self.total_transport_cost {
            return Ok(ordered_json(value)?.len());
        }
        let mut pending = Vec::new();
        referenced(value, &mut pending);
        let mut seen = BTreeSet::new();
        let mut bytes = ordered_json(value)?.len() + 2;
        while let Some(id) = pending.pop() {
            if !seen.insert(id.clone()) {
                continue;
            }
            let node = self
                .dictionary
                .get(&id)
                .context("harness_shared_reference_missing")?;
            bytes += ordered_json(&json!(id))?.len() + 1 + ordered_json(node)?.len();
            if seen.len() > 1 {
                bytes += 1;
            }
            referenced(node, &mut pending);
        }
        Ok(bytes)
    }

    fn encode(&mut self, v: &Value, allow_ref: bool) -> Result<Value> {
        let id = digest(v)?;
        if allow_ref && self.counts.get(&id).is_some_and(|n| *n > 1) {
            if !self.dictionary.contains_key(&id) {
                let node = self.encode(v, false)?;
                self.dictionary.insert(id.clone(), node);
            }
            return Ok(json!({REF:id}));
        }
        match v {
            Value::Object(o) if reserved(o) => Ok(json!({LITERAL:v})),
            Value::Object(o) => {
                let mut result = Map::new();
                for (k, x) in o {
                    result.insert(k.clone(), self.encode(x, true)?);
                }
                Ok(Value::Object(result))
            }
            Value::Array(a) => {
                let ordinary = Value::Array(
                    a.iter()
                        .map(|x| self.encode(x, true))
                        .collect::<Result<_>>()?,
                );
                if a.len() < 4 || !a.iter().all(Value::is_object) {
                    return Ok(ordinary);
                }
                let keys: BTreeSet<_> = a[0].as_object().unwrap().keys().cloned().collect();
                if keys.is_empty()
                    || !a.iter().all(|x| {
                        x.as_object()
                            .unwrap()
                            .keys()
                            .cloned()
                            .collect::<BTreeSet<_>>()
                            == keys
                    })
                {
                    return Ok(ordinary);
                }
                let mut common = Map::new();
                let mut fields = Vec::new();
                for k in keys {
                    if a.iter().all(|x| x[&k] == a[0][&k]) {
                        common.insert(k.clone(), self.encode(&a[0][&k], true)?);
                    } else {
                        fields.push(k);
                    }
                }
                let rows = a
                    .iter()
                    .map(|x| {
                        fields
                            .iter()
                            .map(|k| self.encode(&x[k], true))
                            .collect::<Result<Vec<_>>>()
                    })
                    .collect::<Result<Vec<_>>>()?;
                let mut columns = Vec::new();
                for field in &fields {
                    let values = Value::Array(a.iter().map(|x| x[field].clone()).collect());
                    columns.push(self.encode(&values, true)?);
                }
                let by_columns = json!({ROWS:{"common":common,"fields":fields,"columns":columns,"count":a.len()}});
                let by_rows = json!({ROWS:{"common":common,"fields":fields,"rows":rows}});
                let packed =
                    if self.transport_bytes(&by_columns)? < self.transport_bytes(&by_rows)? {
                        by_columns
                    } else {
                        by_rows
                    };
                Ok(
                    if self.transport_bytes(&packed)? < self.transport_bytes(&ordinary)? {
                        packed
                    } else {
                        ordinary
                    },
                )
            }
            _ => Ok(v.clone()),
        }
    }
}

fn referenced(v: &Value, ids: &mut Vec<String>) {
    match v {
        Value::Object(o) if o.len() == 1 && o.contains_key(LITERAL) => {}
        Value::Object(o) if o.len() == 1 && o.contains_key(REF) => {
            if let Some(id) = o[REF].as_str() {
                ids.push(id.into());
            }
        }
        Value::Object(o) => {
            for x in o.values() {
                referenced(x, ids);
            }
        }
        Value::Array(a) => {
            for x in a {
                referenced(x, ids);
            }
        }
        _ => {}
    }
}

/// Measure the eventual value before allocating it. Codec containers are not
/// output nodes: tables repeat their common subtree and field names, references
/// contribute their target on every use, and literals retain their whole tree.
fn check_expansion_budget(
    root: &Value,
    dictionary: &Map<String, Value>,
    indexed: bool,
) -> Result<()> {
    #[derive(Clone, Copy)]
    struct Size {
        nodes: usize,
        bytes: usize,
        depth: usize,
        object_len: Option<usize>,
        array_len: Option<usize>,
    }
    impl Size {
        fn checked(self) -> Result<Self> {
            ensure!(
                self.nodes <= MAX_NODES && self.depth <= MAX_DEPTH,
                "harness_context_structure_budget_exceeded"
            );
            ensure!(
                self.bytes <= super::MAX_ARTIFACT_BYTES,
                "harness_context_expansion_budget_exceeded"
            );
            Ok(self)
        }
        fn add(&mut self, child: Self, punctuation: usize) -> Result<()> {
            self.nodes = self
                .nodes
                .checked_add(child.nodes)
                .context("harness_context_structure_budget_exceeded")?;
            self.bytes = self
                .bytes
                .checked_add(child.bytes)
                .and_then(|n| n.checked_add(punctuation))
                .context("harness_context_expansion_budget_exceeded")?;
            self.depth = self.depth.max(child.depth + 1);
            self.checked()?;
            Ok(())
        }
    }
    // Count JSON escaping without allocating an escaped copy of a large key or
    // string. UTF-8 bytes other than JSON controls are written verbatim.
    fn string_bytes(text: &str) -> Result<usize> {
        let mut bytes = 2_usize;
        for byte in text.bytes() {
            let width = match byte {
                b'"' | b'\\' | b'\x08' | b'\x0c' | b'\n' | b'\r' | b'\t' => 2,
                0..=31 => 6,
                _ => 1,
            };
            bytes = bytes
                .checked_add(width)
                .context("harness_context_expansion_budget_exceeded")?;
            ensure!(
                bytes <= super::MAX_ARTIFACT_BYTES,
                "harness_context_expansion_budget_exceeded"
            );
        }
        Ok(bytes)
    }
    fn measure(
        v: &Value,
        dictionary: &Map<String, Value>,
        active: &mut BTreeSet<String>,
        depth: usize,
        literal: bool,
        indexed: bool,
    ) -> Result<Size> {
        ensure!(
            depth <= MAX_DEPTH,
            "harness_context_structure_budget_exceeded"
        );
        if !literal && let Some(o) = v.as_object() {
            if o.len() == 1 && o.contains_key(LITERAL) {
                // Do not interpret reserved-looking user data within a literal.
                // Its raw tree has its own output-relative depth. Reference
                // wrappers are not part of that tree; parents add its height.
                return measure(&o[LITERAL], dictionary, active, 0, true, indexed);
            }
            if o.len() == 1 && o.contains_key(REF) {
                let id = o[REF]
                    .as_str()
                    .context("invalid_harness_shared_reference")?;
                ensure!(active.insert(id.into()), "harness_shared_context_cycle");
                let size = measure(
                    dictionary
                        .get(id)
                        .context("harness_shared_reference_missing")?,
                    dictionary,
                    active,
                    depth + usize::from(!indexed),
                    false,
                    indexed,
                )?;
                active.remove(id);
                return Ok(size);
            }
            if o.len() == 1 && o.contains_key(ROWS) {
                let table = &o[ROWS];
                let common = measure(
                    &table["common"],
                    dictionary,
                    active,
                    depth + 1,
                    false,
                    indexed,
                )?;
                let common_len = common.object_len.context("invalid_harness_context_rows")?;
                let fields = table["fields"]
                    .as_array()
                    .context("invalid_harness_context_rows")?;
                ensure!(
                    fields.len() <= MAX_NODES,
                    "harness_context_structure_budget_exceeded"
                );
                let mut field_bytes = 0_usize;
                let mut seen = BTreeSet::new();
                for (index, field) in fields.iter().enumerate() {
                    let name = field.as_str().context("invalid_harness_context_rows")?;
                    ensure!(seen.insert(name), "invalid_harness_context_rows");
                    field_bytes = field_bytes
                        .checked_add(string_bytes(name)?)
                        .and_then(|n| n.checked_add(1 + usize::from(common_len > 0 || index > 0)))
                        .context("harness_context_expansion_budget_exceeded")?;
                }
                let columns = table.get("columns");
                let rows = if columns.is_some() {
                    ensure!(table.get("rows").is_none(), "invalid_harness_context_rows");
                    None
                } else {
                    Some(
                        table["rows"]
                            .as_array()
                            .context("invalid_harness_context_rows")?,
                    )
                };
                let count = match rows {
                    Some(rows) => rows.len(),
                    None => table["count"]
                        .as_u64()
                        .filter(|n| *n <= MAX_NODES as u64)
                        .context("invalid_harness_context_rows")?
                        as usize,
                };
                let mut size = Size {
                    nodes: common
                        .nodes
                        .checked_mul(count)
                        .and_then(|n| n.checked_add(1))
                        .context("harness_context_structure_budget_exceeded")?,
                    bytes: common
                        .bytes
                        .checked_add(field_bytes)
                        .and_then(|n| n.checked_mul(count))
                        .and_then(|n| n.checked_add(2 + count.saturating_sub(1)))
                        .context("harness_context_expansion_budget_exceeded")?,
                    depth: if count == 0 { 0 } else { common.depth + 1 },
                    object_len: None,
                    array_len: Some(count),
                }
                .checked()?;
                if let Some(columns) = columns {
                    let columns = columns.as_array().context("invalid_harness_context_rows")?;
                    ensure!(
                        columns.len() == fields.len(),
                        "invalid_harness_context_rows"
                    );
                    for column in columns {
                        let values =
                            measure(column, dictionary, active, depth + 1, false, indexed)?;
                        ensure!(
                            values.array_len == Some(count),
                            "invalid_harness_context_rows"
                        );
                        // The column's array container becomes row field values;
                        // those values are moved rather than copied by the decoder.
                        size.nodes = size
                            .nodes
                            .checked_add(values.nodes - 1)
                            .context("harness_context_structure_budget_exceeded")?;
                        size.bytes = size
                            .bytes
                            .checked_add(values.bytes - (2 + count.saturating_sub(1)))
                            .context("harness_context_expansion_budget_exceeded")?;
                        if count > 0 {
                            size.depth = size.depth.max(values.depth + 1);
                        }
                        size.checked()?;
                    }
                } else {
                    for row in rows.unwrap() {
                        let row = row.as_array().context("invalid_harness_context_rows")?;
                        ensure!(row.len() == fields.len(), "invalid_harness_context_rows");
                        for value in row {
                            let value =
                                measure(value, dictionary, active, depth + 1, false, indexed)?;
                            size.nodes = size
                                .nodes
                                .checked_add(value.nodes)
                                .context("harness_context_structure_budget_exceeded")?;
                            size.bytes = size
                                .bytes
                                .checked_add(value.bytes)
                                .context("harness_context_expansion_budget_exceeded")?;
                            size.depth = size.depth.max(value.depth + 2);
                            size.checked()?;
                        }
                    }
                }
                return Ok(size);
            }
        }
        let mut size = Size {
            nodes: 1,
            bytes: 2,
            depth: 0,
            object_len: v.as_object().map(Map::len),
            array_len: v.as_array().map(Vec::len),
        };
        match v {
            Value::Object(items) => {
                for (index, (key, value)) in items.iter().enumerate() {
                    size.add(
                        measure(value, dictionary, active, depth + 1, literal, indexed)?,
                        string_bytes(key)? + 1 + usize::from(index > 0),
                    )?;
                }
            }
            Value::Array(items) => {
                for (index, value) in items.iter().enumerate() {
                    size.add(
                        measure(value, dictionary, active, depth + 1, literal, indexed)?,
                        usize::from(index > 0),
                    )?;
                }
            }
            Value::String(text) => size.bytes = string_bytes(text)?,
            _ => size.bytes = canonical_json(v)?.len(),
        }
        size.checked()
    }
    measure(root, dictionary, &mut BTreeSet::new(), 0, false, indexed)?;
    Ok(())
}

/// The indexed protocol bounds its encoded form as well as its expansion. This
/// preflight never interprets source strings or literal escapes as instructions.
fn indexed_input_budget(view: &Value) -> Result<()> {
    fn walk(value: &Value, nodes: &mut usize, depth: usize) -> Result<()> {
        *nodes += 1;
        ensure!(
            *nodes <= MAX_NODES && depth <= MAX_DEPTH,
            "harness_context_structure_budget_exceeded"
        );
        match value {
            Value::Object(o) => {
                for child in o.values() {
                    walk(child, nodes, depth + 1)?;
                }
            }
            Value::Array(a) => {
                for child in a {
                    walk(child, nodes, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    walk(view, &mut 0, 0)?;
    ensure!(
        ordered_json(view)?.len() <= super::MAX_ARTIFACT_BYTES,
        "harness_context_expansion_budget_exceeded"
    );
    Ok(())
}

fn indexed_dictionary(view: &Value, dictionary: &Map<String, Value>) -> Result<()> {
    for index in 0..dictionary.len() {
        ensure!(
            dictionary.contains_key(&index.to_string()),
            "invalid_harness_indexed_dictionary"
        );
    }
    let mut pending = Vec::new();
    referenced(&view["root"], &mut pending);
    let mut reached = BTreeSet::new();
    while let Some(id) = pending.pop() {
        let index = id
            .parse::<usize>()
            .context("invalid_harness_indexed_reference")?;
        ensure!(index.to_string() == id, "invalid_harness_indexed_reference");
        let node = dictionary
            .get(&id)
            .context("harness_shared_reference_missing")?;
        if reached.insert(id) {
            referenced(node, &mut pending);
        }
    }
    ensure!(
        reached.len() == dictionary.len(),
        "harness_indexed_dictionary_unused"
    );
    Ok(())
}

/// A chain of codec aliases is not output nesting. Resolve it iteratively with
/// path compression before recursive tree work, so a shallow, bounded wire
/// cannot exhaust the stack or acquire an undocumented reference-depth limit.
/// This internal form does not alter the recorded view or its identities.
fn indexed_resolve_aliases(
    root: &Value,
    dictionary: &Map<String, Value>,
) -> Result<(Value, Map<String, Value>)> {
    let mut resolved: BTreeMap<String, String> = BTreeMap::new();
    for first in dictionary.keys() {
        let mut id = first.clone();
        let mut path = BTreeSet::new();
        let terminal = loop {
            if let Some(target) = resolved.get(&id) {
                break target.clone();
            }
            ensure!(path.insert(id.clone()), "harness_shared_context_cycle");
            let node = dictionary
                .get(&id)
                .context("harness_shared_reference_missing")?;
            if let Some(o) = node.as_object()
                && o.len() == 1
                && o.contains_key(REF)
            {
                id = o[REF]
                    .as_str()
                    .context("invalid_harness_shared_reference")?
                    .into();
            } else {
                break id;
            }
        };
        for id in path {
            resolved.insert(id, terminal.clone());
        }
    }
    fn replace(value: &Value, resolved: &BTreeMap<String, String>) -> Result<Value> {
        Ok(match value {
            Value::Object(o) if o.len() == 1 && o.contains_key(LITERAL) => value.clone(),
            Value::Object(o) if o.len() == 1 && o.contains_key(REF) => {
                let id = o[REF]
                    .as_str()
                    .context("invalid_harness_shared_reference")?;
                json!({REF:resolved.get(id).context("harness_shared_reference_missing")?})
            }
            Value::Object(o) => Value::Object(
                o.iter()
                    .map(|(k, v)| Ok((k.clone(), replace(v, resolved)?)))
                    .collect::<Result<_>>()?,
            ),
            Value::Array(a) => Value::Array(
                a.iter()
                    .map(|v| replace(v, resolved))
                    .collect::<Result<_>>()?,
            ),
            _ => value.clone(),
        })
    }
    let entries = dictionary
        .iter()
        .filter(|(id, _)| resolved[*id] == **id)
        .map(|(id, value)| Ok((id.clone(), replace(value, &resolved)?)))
        .collect::<Result<_>>()?;
    Ok((replace(root, &resolved)?, entries))
}

/// Expands only codec-owned tokens; matching user objects were escaped. Cycle,
/// duplicate-column, dangling-reference and amplified-output inputs are rejected.
pub fn expand(view: &Value) -> Result<Value> {
    if let Some(text) = view.as_str() {
        ensure!(
            text.len() <= super::MAX_ARTIFACT_BYTES,
            "harness_context_expansion_budget_exceeded"
        );
        let parsed = strict_json(text.as_bytes()).context("invalid_harness_shared_context")?;
        // Only one explicit JSON-text envelope. Arbitrary condition text is
        // handled separately, never recursively interpreted as instructions.
        ensure!(parsed.is_object(), "invalid_harness_shared_context");
        return expand(&parsed);
    }
    let indexed = view["schema"] == INDEXED_VIEW_SCHEMA;
    ensure!(
        (view["schema"] == VIEW_SCHEMA || indexed)
            && view["coverage"] == "all_normalized_semantic_values",
        "invalid_harness_shared_context"
    );
    let dictionary = view["dictionary"]
        .as_object()
        .context("invalid_harness_shared_context")?;
    ensure!(
        dictionary.len() <= MAX_NODES,
        "harness_context_structure_budget_exceeded"
    );
    if indexed {
        indexed_input_budget(view)?;
        indexed_dictionary(view, dictionary)?;
    }
    let resolved = if indexed {
        Some(indexed_resolve_aliases(&view["root"], dictionary)?)
    } else {
        None
    };
    let (root, dictionary) = resolved
        .as_ref()
        .map(|(root, dictionary)| (root, dictionary))
        .unwrap_or((&view["root"], dictionary));
    // Reserve the complete output budget before any table/literal subtree is
    // expanded. New indexed views count actual output, not traversal work;
    // legacy views retain their historical conservative operation guards.
    check_expansion_budget(root, dictionary, indexed)?;
    fn decode(
        v: &Value,
        dict: &Map<String, Value>,
        active: &mut BTreeSet<String>,
        nodes: &mut usize,
        copied_bytes: &mut usize,
        depth: usize,
        indexed: bool,
    ) -> Result<Value> {
        *nodes += 1;
        ensure!(
            (indexed || *nodes <= MAX_NODES) && depth <= MAX_DEPTH,
            "harness_context_structure_budget_exceeded"
        );
        let leaf_bytes = match v {
            Value::String(s) => s.len(),
            Value::Object(o) => o.keys().map(String::len).sum(),
            _ => 16,
        };
        *copied_bytes = copied_bytes
            .checked_add(leaf_bytes)
            .context("harness_context_expansion_budget_exceeded")?;
        ensure!(
            indexed || *copied_bytes <= super::MAX_ARTIFACT_BYTES,
            "harness_context_expansion_budget_exceeded"
        );
        match v {
            Value::Object(o) if o.len() == 1 && o.contains_key(LITERAL) => {
                *copied_bytes = copied_bytes
                    .checked_add(canonical_json(&o[LITERAL])?.len())
                    .context("harness_context_expansion_budget_exceeded")?;
                ensure!(
                    indexed || *copied_bytes <= super::MAX_ARTIFACT_BYTES,
                    "harness_context_expansion_budget_exceeded"
                );
                Ok(o[LITERAL].clone())
            }
            Value::Object(o) if o.len() == 1 && o.contains_key(REF) => {
                let id = o[REF]
                    .as_str()
                    .context("invalid_harness_shared_reference")?;
                ensure!(active.insert(id.into()), "harness_shared_context_cycle");
                let value = decode(
                    dict.get(id).context("harness_shared_reference_missing")?,
                    dict,
                    active,
                    nodes,
                    copied_bytes,
                    depth + usize::from(!indexed),
                    indexed,
                )?;
                active.remove(id);
                ensure!(
                    indexed || digest(&value)? == id,
                    "harness_shared_reference_digest_mismatch"
                );
                Ok(value)
            }
            Value::Object(o) if o.len() == 1 && o.contains_key(ROWS) => {
                let table = &o[ROWS];
                let common = decode(
                    &table["common"],
                    dict,
                    active,
                    nodes,
                    copied_bytes,
                    depth + 1,
                    indexed,
                )?;
                let common_bytes = canonical_json(&common)?.len();
                let common = common.as_object().context("invalid_harness_context_rows")?;
                let fields = table["fields"]
                    .as_array()
                    .context("invalid_harness_context_rows")?;
                let mut seen: BTreeSet<String> = common.keys().cloned().collect();
                for f in fields {
                    ensure!(
                        seen.insert(f.as_str().context("invalid_harness_context_rows")?.into()),
                        "invalid_harness_context_rows"
                    );
                }
                let mut result = Vec::new();
                if table.get("columns").is_some() {
                    ensure!(table.get("rows").is_none(), "invalid_harness_context_rows");
                    let count = table["count"]
                        .as_u64()
                        .filter(|n| *n <= MAX_NODES as u64)
                        .context("invalid_harness_context_rows")?
                        as usize;
                    let encoded = table["columns"]
                        .as_array()
                        .context("invalid_harness_context_rows")?;
                    ensure!(
                        encoded.len() == fields.len(),
                        "invalid_harness_context_rows"
                    );
                    let mut columns = Vec::new();
                    for c in encoded {
                        let Value::Array(values) =
                            decode(c, dict, active, nodes, copied_bytes, depth + 1, indexed)?
                        else {
                            anyhow::bail!("invalid_harness_context_rows");
                        };
                        ensure!(values.len() == count, "invalid_harness_context_rows");
                        columns.push(values);
                    }
                    for index in 0..count {
                        *copied_bytes = copied_bytes
                            .checked_add(common_bytes)
                            .context("harness_context_expansion_budget_exceeded")?;
                        ensure!(
                            indexed || *copied_bytes <= super::MAX_ARTIFACT_BYTES,
                            "harness_context_expansion_budget_exceeded"
                        );
                        let mut row = common.clone();
                        for (field, column) in fields.iter().zip(columns.iter_mut()) {
                            row.insert(
                                field.as_str().unwrap().into(),
                                std::mem::take(&mut column[index]),
                            );
                        }
                        result.push(Value::Object(row));
                    }
                    return Ok(Value::Array(result));
                }
                let rows = table["rows"]
                    .as_array()
                    .context("invalid_harness_context_rows")?;
                for row in rows {
                    let row = row.as_array().context("invalid_harness_context_rows")?;
                    ensure!(row.len() == fields.len(), "invalid_harness_context_rows");
                    *copied_bytes = copied_bytes
                        .checked_add(common_bytes)
                        .context("harness_context_expansion_budget_exceeded")?;
                    ensure!(
                        indexed || *copied_bytes <= super::MAX_ARTIFACT_BYTES,
                        "harness_context_expansion_budget_exceeded"
                    );
                    let mut value = common.clone();
                    for (f, x) in fields.iter().zip(row) {
                        value.insert(
                            f.as_str().unwrap().into(),
                            decode(x, dict, active, nodes, copied_bytes, depth + 1, indexed)?,
                        );
                    }
                    result.push(Value::Object(value));
                }
                Ok(Value::Array(result))
            }
            Value::Object(o) => {
                let mut value = Map::new();
                for (k, x) in o {
                    value.insert(
                        k.clone(),
                        decode(x, dict, active, nodes, copied_bytes, depth + 1, indexed)?,
                    );
                }
                Ok(Value::Object(value))
            }
            Value::Array(a) => Ok(Value::Array(
                a.iter()
                    .map(|x| decode(x, dict, active, nodes, copied_bytes, depth + 1, indexed))
                    .collect::<Result<_>>()?,
            )),
            _ => Ok(v.clone()),
        }
    }
    let result = decode(
        root,
        dictionary,
        &mut BTreeSet::new(),
        &mut 0,
        &mut 0,
        0,
        indexed,
    )?;
    if indexed {
        // Verify the exact realized tree as well as the pre-allocation proof.
        // Encoded and expanded values each get the same independent budget.
        indexed_input_budget(&result)?;
    }
    ensure!(
        digest(&result)? == view["normalized_sha256"],
        "harness_context_expansion_mismatch"
    );
    Ok(result)
}

pub fn project(full_model_projection: &Value, p: &HarnessContextPolicy) -> Result<(Value, Value)> {
    project_version(full_model_projection, p, true)
}

/// Consolidation shares the codec, not the routing policy's byte contract.
/// All selected trajectories remain present; no episode or fact is selected out.
pub fn project_consolidation(
    full: &Value,
    p: &oe_contracts::ConsolidationContextPolicy,
) -> Result<(Value, Value)> {
    p.validate()
        .map_err(|_| anyhow::anyhow!("invalid_consolidation_context_policy"))?;
    ensure!(
        (28_000..=super::MAX_CONSOLIDATION_BYTES as u64).contains(&p.byte_budget),
        "invalid_consolidation_context_policy"
    );
    let (view, mut record) = project_codec(full, &p.mode, json!(p), true)?;
    record["schema"] = json!("oev2.consolidation-context-transport/1");
    Ok((view, record))
}

/// Verify historical bytes with their recorded encoder, without rewriting or
/// adopting the old result. New work always uses `project` and the current one.
pub fn project_for_archive(
    full_model_projection: &Value,
    p: &HarnessContextPolicy,
    record: &Value,
) -> Result<(Value, Value)> {
    if record["schema"] == "oev2.harness-context-transport/3" {
        ensure!(
            p.schema == INDEXED_POLICY_SCHEMA
                && record["encoder_version"] == INDEXED_ENCODER_VERSION,
            "harness_context_archive_mismatch"
        );
        return project_version(full_model_projection, p, true);
    }
    ensure!(
        p.schema != INDEXED_POLICY_SCHEMA,
        "harness_context_archive_mismatch"
    );
    let current = match record["schema"].as_str() {
        Some("oev2.harness-context-transport/1") if record.get("encoder_version").is_none() => {
            false
        }
        Some("oev2.harness-context-transport/2")
            if record["encoder_version"] == ENCODER_VERSION =>
        {
            true
        }
        _ => anyhow::bail!("harness_context_archive_mismatch"),
    };
    ensure!(
        current || p.schema == POLICY_SCHEMA,
        "harness_context_archive_mismatch"
    );
    project_version(full_model_projection, p, current)
}

fn project_version(
    full_model_projection: &Value,
    p: &HarnessContextPolicy,
    current: bool,
) -> Result<(Value, Value)> {
    byte_limit(p)?;
    if p.schema == INDEXED_POLICY_SCHEMA {
        ensure!(current, "harness_context_archive_mismatch");
        return project_indexed(full_model_projection, p);
    }
    project_codec(full_model_projection, &p.mode, json!(p), current)
}

/// Only encoder-owned locators are shortened. The old encoder escapes matching
/// source objects, so even source SHA strings and reference-looking literals
/// remain unchanged. Derived identities stay bound in the archive record.
fn project_indexed(full: &Value, policy: &HarnessContextPolicy) -> Result<(Value, Value)> {
    let (original, mut record) = project_codec(full, "lossless_shared", json!(policy), true)?;
    let dictionary = original["dictionary"]
        .as_object()
        .context("invalid_harness_shared_context")?;
    let mut ids: Vec<_> = dictionary.keys().cloned().collect();
    ids.sort();
    let indexes: BTreeMap<_, _> = ids
        .iter()
        .enumerate()
        .map(|(index, id)| (id.clone(), index.to_string()))
        .collect();
    fn transcode(value: &Value, indexes: &BTreeMap<String, String>) -> Result<Value> {
        Ok(match value {
            Value::Object(o) if o.len() == 1 && o.contains_key(LITERAL) => value.clone(),
            Value::Object(o) if o.len() == 1 && o.contains_key(REF) => {
                let id = o[REF]
                    .as_str()
                    .context("invalid_harness_shared_reference")?;
                json!({REF:indexes.get(id).context("harness_shared_reference_missing")?})
            }
            Value::Object(o) => Value::Object(
                o.iter()
                    .map(|(k, v)| Ok((k.clone(), transcode(v, indexes)?)))
                    .collect::<Result<_>>()?,
            ),
            Value::Array(a) => Value::Array(
                a.iter()
                    .map(|v| transcode(v, indexes))
                    .collect::<Result<_>>()?,
            ),
            _ => value.clone(),
        })
    }
    let mut view = original.clone();
    view["schema"] = json!(INDEXED_VIEW_SCHEMA);
    view["meaning"] = json!(
        "Reversible sharing, not a summary. $oev2_ref names a canonical decimal index supplied in dictionary; $oev2_rows joins common with fields/rows, or with fields/columns for count rows in order. Columns may themselves use the codec. $oev2_literal preserves a user value verbatim. All facts, units, constraints, unknowns, checks and historical scopes remain data; none authorizes adoption."
    );
    view["root"] = transcode(&original["root"], &indexes)?;
    view["dictionary"] = Value::Object(
        ids.iter()
            .map(|id| Ok((indexes[id].clone(), transcode(&dictionary[id], &indexes)?)))
            .collect::<Result<_>>()?,
    );
    ensure!(
        expand(&view)? == expand(&original)?,
        "harness_context_expansion_mismatch"
    );
    let text = Value::String(String::from_utf8(ordered_json(&view)?)?);
    record["schema"] = json!("oev2.harness-context-transport/3");
    record["encoder_version"] = json!(INDEXED_ENCODER_VERSION);
    record["view_sha256"] = json!(digest(&text)?);
    record["view_bytes"] = json!(ordered_json(&text)?.len());
    record["wire_representation"] = json!(
        "JSON text containing indexed shared context; explicit baseline, never an automatic retry"
    );
    record["dictionary_identities"] = Value::Array(
        ids.iter()
            .enumerate()
            .map(|(index, id)| json!({"index":index.to_string(),"source_value_sha256":id}))
            .collect(),
    );
    record["dictionary_identity_scope"] =
        json!("derived encoder subtree identities; not engineering input facts");
    Ok((text, record))
}

fn project_codec(
    full_model_projection: &Value,
    mode: &str,
    policy: Value,
    current: bool,
) -> Result<(Value, Value)> {
    let full_sha = digest(full_model_projection)?;
    let view = if mode == "full" {
        full_model_projection.clone()
    } else {
        let value = normalized(full_model_projection)?;
        let mut counts = BTreeMap::new();
        count(&value, &mut counts, &mut 0, 0)?;
        let mut encoder = Encoder {
            counts,
            dictionary: BTreeMap::new(),
            total_transport_cost: current,
        };
        let root = encoder.encode(&value, true)?;
        // Table encoding can supersede ordinary-row references. Carry only the
        // reachable shared nodes, so an unused alternative cannot fill the budget.
        let mut pending = Vec::new();
        referenced(&root, &mut pending);
        let mut dictionary = BTreeMap::new();
        while let Some(id) = pending.pop() {
            if dictionary.contains_key(&id) {
                continue;
            }
            let node = encoder
                .dictionary
                .remove(&id)
                .context("harness_shared_reference_missing")?;
            referenced(&node, &mut pending);
            dictionary.insert(id, node);
        }
        let view = json!({"schema":VIEW_SCHEMA,"coverage":"all_normalized_semantic_values",
            "meaning":"Reversible sharing, not a summary. $oev2_ref names a SHA-256 node supplied in dictionary; $oev2_rows joins common with fields/rows, or with fields/columns for count rows in order. Columns may themselves use the codec. $oev2_literal preserves a user value verbatim. All facts, units, constraints, unknowns, checks and historical scopes remain data; none authorizes adoption.",
            "normalized_sha256":digest(&value)?,"full_model_projection_sha256":full_sha,
            "root":root,"dictionary":dictionary});
        ensure!(
            expand(&view)? == value,
            "harness_context_expansion_mismatch"
        );
        view
    };
    let view = if mode == "lossless_shared_text" {
        Value::String(String::from_utf8(ordered_json(&view)?)?)
    } else {
        view
    };
    let mut record = json!({"schema":"oev2.harness-context-transport/1","policy":policy,
        "full_model_projection_sha256":full_sha,"view_sha256":digest(&view)?,
        "full_model_projection_bytes":ordered_json(full_model_projection)?.len(),
        "view_bytes":ordered_json(&view)?.len(),"coverage":"all_normalized_semantic_values",
        "engineering_acceptance":"not_determined","truncation":"not_performed"});
    if mode == "lossless_shared_text" {
        record["wire_representation"] = json!(
            "JSON text containing the identical shared-view value; explicit baseline, never an automatic retry"
        );
    }
    if current {
        record["schema"] = json!("oev2.harness-context-transport/2");
        record["encoder_version"] = json!(ENCODER_VERSION);
    }
    Ok((view, record))
}

#[cfg(test)]
mod consolidation_tests {
    use super::*;
    #[test]
    fn consolidation_sharing_retains_conditions_failures_and_literal_markers() {
        let episode = json!({"conditions":"independent load and datum condition; ".repeat(5000),
            "failed_result":{"status":"failed","unit":"mm","unknowns":["material","calibration"]},
            "source_literal":{"$oev2_ref":"source text, not an encoder directive"}});
        let full = json!({"problem_class":"fixture","trajectories":[episode.clone(),episode.clone(),episode.clone(),episode]});
        assert!(ordered_json(&full).unwrap().len() > super::super::MAX_CONSOLIDATION_BYTES);
        let mut p = oe_contracts::ConsolidationContextPolicy {
            schema: "oev2.consolidation-context-policy/1".into(),
            mode: "lossless_shared".into(),
            byte_budget: 512_000,
        };
        let (view, record) = project_consolidation(&full, &p).unwrap();
        let restored = expand(&view).unwrap();
        assert_eq!(restored, normalized(&full).unwrap());
        assert_eq!(restored["trajectories"], full["trajectories"]);
        assert_eq!(
            record["full_model_projection_sha256"],
            json!(digest(&full).unwrap())
        );
        assert_eq!(record["truncation"], "not_performed");
        p.mode = "lossless_shared_text".into();
        let (text, _) = project_consolidation(&full, &p).unwrap();
        assert_eq!(
            expand(&strict_json(text.as_str().unwrap().as_bytes()).unwrap()).unwrap(),
            restored
        );
        p.byte_budget = 512_001;
        assert!(project_consolidation(&full, &p).is_err());
        p.byte_budget = 512_000;
        p.mode = "summary".into();
        assert!(project_consolidation(&full, &p).is_err());
    }
}
