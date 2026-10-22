//! Validation helpers for SKOS graph relationships (`semantica/utils/skos.py`).

use indexmap::{IndexMap, IndexSet};
use serde_json::Value;

use crate::exceptions::SemanticaError;
use crate::pycompat::fmt::{py_str, strip};

const HIERARCHY_EDGE_TYPES: &[&str] = &["skos:broader", "skos:narrower"];
const TYPE_KEYS: &[&str] = &["type", "edge_type", "relationship", "predicate", "relation"];

fn hierarchy_type(value: Option<&Value>) -> Option<&str> {
    match value {
        Some(Value::String(s)) if HIERARCHY_EDGE_TYPES.contains(&s.as_str()) => Some(s),
        _ => None,
    }
}

/// `is_skos_hierarchy_edge(edge)`: whether a mapping edge uses `skos:broader` or
/// `skos:narrower` under `type`, `edge_type`, `relationship`, `predicate` or `relation`.
/// Non-mappings are never hierarchy edges.
pub fn is_skos_hierarchy_edge(edge: &Value) -> bool {
    match edge {
        Value::Object(map) => TYPE_KEYS
            .iter()
            .any(|k| hierarchy_type(map.get(*k)).is_some()),
        _ => false,
    }
}

/// Normalize a SKOS hierarchy edge to a `(child, parent)` pair.
fn child_parent(edge: &Value) -> Option<(String, String)> {
    let Value::Object(map) = edge else {
        return None;
    };
    let edge_type = TYPE_KEYS.iter().find_map(|k| hierarchy_type(map.get(*k)))?;
    let endpoint = |key: &str, fallback: &str| -> Option<String> {
        let raw = match map.get(key) {
            Some(value) => value,
            None => map.get(fallback)?,
        };
        if raw.is_null() {
            return None;
        }
        let text = strip(&py_str(raw)).to_string();
        (!text.is_empty()).then_some(text)
    };
    let source = endpoint("source", "source_id")?;
    let target = endpoint("target", "target_id")?;
    Some(if edge_type == "skos:broader" {
        (source, target)
    } else {
        (target, source)
    })
}

/// `validate_skos_hierarchy(new_edges, existing_edges=())`: `ValueError` when adding
/// `new_edges` would introduce a cycle in the `skos:broader`/`skos:narrower` hierarchy.
///
/// Only concepts touched by `new_edges` are checked, so a cycle confined to `existing_edges`
/// does not block an unrelated write. Concepts are visited in insertion order (Python visits a
/// set, so which concept the message names can differ between runs there).
pub fn validate_skos_hierarchy(
    new_edges: &[Value],
    existing_edges: Option<&[Value]>,
) -> Result<(), SemanticaError> {
    let mut parents: IndexMap<String, IndexSet<String>> = IndexMap::new();
    for edge in existing_edges.unwrap_or(&[]) {
        if let Some((child, parent)) = child_parent(edge) {
            parents.entry(child).or_default().insert(parent);
        }
    }
    let mut touched: IndexSet<String> = IndexSet::new();
    for edge in new_edges {
        if let Some((child, parent)) = child_parent(edge) {
            parents
                .entry(child.clone())
                .or_default()
                .insert(parent.clone());
            touched.insert(child);
            touched.insert(parent);
        }
    }

    let mut visited: IndexSet<String> = IndexSet::new();
    for start in &touched {
        if visited.contains(start) {
            continue;
        }
        // Iterative depth-first search; `visiting` holds the current path.
        let mut visiting: IndexSet<String> = IndexSet::new();
        let mut stack: Vec<(String, usize)> = vec![(start.clone(), 0)];
        visiting.insert(start.clone());
        while let Some((concept, next_index)) = stack.last().cloned() {
            let next = parents
                .get(&concept)
                .and_then(|ps| ps.get_index(next_index))
                .cloned();
            match next {
                Some(parent) => {
                    stack.last_mut().expect("non-empty stack").1 += 1;
                    if visiting.contains(&parent) {
                        return Err(SemanticaError::Value(format!(
                            "SKOS hierarchy contains a cycle involving '{parent}'."
                        )));
                    }
                    if !visited.contains(&parent) {
                        visiting.insert(parent.clone());
                        stack.push((parent, 0));
                    }
                }
                None => {
                    stack.pop();
                    visiting.shift_remove(&concept);
                    visited.insert(concept);
                }
            }
        }
    }
    Ok(())
}
