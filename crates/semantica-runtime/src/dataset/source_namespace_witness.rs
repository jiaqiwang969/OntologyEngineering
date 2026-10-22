//! Structural/relational checks; a consistent witness is not execution provenance.
use anyhow::{Result, ensure};
use semantica_contracts::{SemanticSourceNamespaceManager4, SemanticSourceNamespaceState4};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn validate_namespace_witness(
    w: &SemanticSourceNamespaceState4,
    max_bytes: usize,
) -> Result<()> {
    validate_bounded(w, max_bytes, None)
}
pub(super) fn validate_namespace_witness_until(
    w: &SemanticSourceNamespaceState4,
    max_bytes: usize,
    at: std::time::Instant,
) -> Result<()> {
    validate_bounded(w, max_bytes, Some(at))
}
fn deadline(at: Option<std::time::Instant>) -> Result<()> {
    ensure!(
        at.is_none_or(|at| std::time::Instant::now() < at),
        "source_namespace_witness_deadline"
    );
    Ok(())
}
fn validate_bounded(
    w: &SemanticSourceNamespaceState4,
    max_bytes: usize,
    at: Option<std::time::Instant>,
) -> Result<()> {
    deadline(at)?;
    if let Some(at) = at {
        super::source_storage_witness::size_until(w, max_bytes.min(32 * 1024 * 1024), at)?;
    } else {
        super::source_storage_witness::size(w, max_bytes.min(32 * 1024 * 1024))?;
    }
    ensure!(w.validate().is_ok(), "source_namespace_witness_schema");
    let mut prefixes = BTreeMap::new();
    let mut namespaces = BTreeSet::new();
    for b in &w.bindings {
        deadline(at)?;
        ensure!(
            prefixes
                .insert(b.prefix.as_str(), b.namespace.as_str())
                .is_none()
                && namespaces.insert(b.namespace.as_str()),
            "source_namespace_witness_binding_duplicate"
        );
    }
    if !w.bindings.is_empty() {
        ensure!(
            w.bindings.len() >= super::source_namespace::DEFAULTS.len(),
            "source_namespace_witness_defaults"
        );
        for (b, (p, n)) in w.bindings.iter().zip(super::source_namespace::DEFAULTS) {
            ensure!(
                b.prefix == *p && b.namespace == *n,
                "source_namespace_witness_defaults"
            );
        }
    }
    for manager in [&w.default_manager, &w.dataset_manager]
        .into_iter()
        .flatten()
    {
        ensure!(
            !w.bindings.is_empty(),
            "source_namespace_witness_manager_bindings"
        );
        deadline(at)?;
        check_manager(manager, &prefixes, at)?;
    }
    deadline(at)?;
    Ok(())
}
fn check_manager(
    m: &SemanticSourceNamespaceManager4,
    prefixes: &BTreeMap<&str, &str>,
    at: Option<std::time::Instant>,
) -> Result<()> {
    ensure!(
        !m.nodes.is_empty() && m.nodes.len() <= 100_000,
        "source_namespace_witness_node_budget"
    );
    ensure!(
        m.strict_cache.is_empty(),
        "source_namespace_witness_strict_cache_unreceived"
    );
    let mut names: Vec<Option<&str>> = vec![None; m.nodes.len()];
    names[0] = Some("");
    let mut next = 1usize;
    let mut stack = vec![(0usize, 0usize)];
    while let Some((parent, cursor)) = stack.last_mut() {
        deadline(at)?;
        let node = *parent;
        if *cursor == 0 {
            let mut siblings = BTreeSet::new();
            for e in &m.nodes[node].children {
                deadline(at)?;
                ensure!(
                    siblings.insert(e.namespace.as_str()),
                    "source_namespace_witness_edge_duplicate"
                );
            }
            let sorted: Vec<_> = siblings.into_iter().collect();
            ensure!(
                !sorted.windows(2).any(|v| v[1].starts_with(v[0])),
                "source_namespace_witness_sibling_prefix"
            );
        }
        let Some(edge) = m.nodes[node].children.get(*cursor) else {
            stack.pop();
            continue;
        };
        *cursor += 1;
        let target = usize::try_from(edge.target)
            .ok()
            .filter(|i| *i < m.nodes.len())
            .ok_or_else(|| anyhow::anyhow!("source_namespace_witness_target"))?;
        ensure!(
            names[target].is_none(),
            "source_namespace_witness_cycle_or_alias"
        );
        ensure!(
            target == next,
            "source_namespace_witness_noncanonical_nodes"
        );
        next += 1;
        let parent_name = names[node].unwrap();
        ensure!(
            edge.namespace.len() > parent_name.len() && edge.namespace.starts_with(parent_name),
            "source_namespace_witness_prefix_relation"
        );
        names[target] = Some(&edge.namespace);
        stack.push((target, 0));
    }
    ensure!(
        next == m.nodes.len(),
        "source_namespace_witness_unreachable_node"
    );
    let names_set: BTreeSet<_> = names.iter().flatten().copied().collect();
    ensure!(
        super::source_namespace::DEFAULTS
            .iter()
            .all(|(_, n)| names_set.contains(n)),
        "source_namespace_witness_default_trie"
    );
    let mut last = None;
    for s in &m.split_cache {
        deadline(at)?;
        ensure!(
            last.is_none_or(|v: &str| v < s.namespace.as_str()),
            "source_namespace_witness_split_order"
        );
        last = Some(s.namespace.as_str());
        ensure!(
            usize::try_from(s.target)
                .ok()
                .and_then(|i| names.get(i))
                .copied()
                .flatten()
                == Some(s.namespace.as_str()),
            "source_namespace_witness_split_reference"
        );
    }
    let mut last = None;
    for q in &m.qname_cache {
        deadline(at)?;
        ensure!(
            last.is_none_or(|v: &str| v < q.uri.as_str()),
            "source_namespace_witness_qname_order"
        );
        last = Some(q.uri.as_str());
        ensure!(
            prefixes.get(q.prefix.as_str()).copied() == Some(q.namespace.as_str()),
            "source_namespace_witness_qname_binding"
        );
        ensure!(
            q.uri.strip_prefix(&q.namespace) == Some(q.local.as_str()),
            "source_namespace_witness_qname_local"
        );
        ensure!(
            names_set.contains(q.namespace.as_str()),
            "source_namespace_witness_qname_trie"
        );
    }
    Ok(())
}
