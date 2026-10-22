//! Read actual retained namespace state; never initialize a manager here.
use super::*;
use semantica_contracts::{
    SemanticSourceNamespaceBinding4 as Binding, SemanticSourceNamespaceEdge4 as Edge,
    SemanticSourceNamespaceManager4 as State, SemanticSourceNamespaceNode4 as Node,
    SemanticSourceNamespaceQName4 as QName, SemanticSourceNamespaceSplit4 as Split,
    SemanticSourceNamespaceState4 as Witness,
};

struct CaptureBudget {
    remaining: usize,
    deadline: Option<std::time::Instant>,
}
impl CaptureBudget {
    fn take(&mut self, chars: usize, overhead: usize) -> Result<()> {
        ensure!(
            self.deadline
                .is_none_or(|at| std::time::Instant::now() < at),
            "source_namespace_capture_deadline"
        );
        let bytes = chars
            .checked_mul(6)
            .and_then(|n| n.checked_add(overhead))
            .ok_or_else(|| anyhow::anyhow!("source_namespace_capture_budget"))?;
        self.remaining = self
            .remaining
            .checked_sub(bytes)
            .ok_or_else(|| anyhow::anyhow!("source_namespace_capture_budget"))?;
        Ok(())
    }
}
impl Manager {
    fn capture(&self, budget: &mut CaptureBudget) -> Result<State> {
        budget.take(0, 256)?;
        ensure!(
            !self.trie.is_empty(),
            "source_namespace_capture_missing_root"
        );
        let mut ids = BTreeMap::from([(0usize, 0u64)]);
        let mut nodes = vec![Node {
            children: Vec::new(),
        }];
        let mut stack = vec![(0usize, 0usize)];
        // Iterative original child-order DFS preserves references without using
        // Rust arena allocation order as cross-language identity.
        while let Some((old, cursor)) = stack.last_mut() {
            let parent = *old;
            let Some(&child) = self.trie[parent].children.get(*cursor) else {
                stack.pop();
                continue;
            };
            *cursor += 1;
            let node = self
                .trie
                .get(child)
                .ok_or_else(|| anyhow::anyhow!("source_namespace_capture_link"))?;
            budget.take(node.value.len(), 128)?;
            let fresh = !ids.contains_key(&child);
            let target = if let Some(&id) = ids.get(&child) {
                id
            } else {
                let id = nodes.len() as u64;
                ids.insert(child, id);
                nodes.push(Node {
                    children: Vec::new(),
                });
                id
            };
            nodes[ids[&parent] as usize].children.push(Edge {
                namespace: node.value.clone(),
                target,
            });
            if fresh {
                stack.push((child, 0));
            }
        }
        ensure!(
            ids.len() == self.trie.len(),
            "source_namespace_capture_unreachable_node"
        );
        let mut split_cache = Vec::new();
        for (namespace, target) in &self.strie {
            budget.take(namespace.len(), 96)?;
            let target = *ids
                .get(target)
                .ok_or_else(|| anyhow::anyhow!("source_namespace_capture_split"))?;
            split_cache.push(Split {
                namespace: namespace.clone(),
                target,
            });
        }
        let mut qname_cache = Vec::new();
        for (uri, [prefix, namespace, local]) in &self.cache {
            budget.take(
                uri.len() + prefix.len() + namespace.len() + local.len(),
                160,
            )?;
            qname_cache.push(QName {
                uri: uri.clone(),
                prefix: prefix.clone(),
                namespace: namespace.clone(),
                local: local.clone(),
            });
        }
        Ok(State {
            nodes,
            split_cache,
            qname_cache,
            strict_cache: Vec::new(),
        })
    }
}
impl NamespaceState {
    pub(super) fn capture(&self, max_bytes: usize) -> Result<Witness> {
        self.capture_bounded(max_bytes, None)
    }
    /// Receives an existing absolute host deadline, never grants a fresh budget.
    pub(in crate::dataset) fn capture_until(
        &self,
        max_bytes: usize,
        at: std::time::Instant,
    ) -> Result<Witness> {
        self.capture_bounded(max_bytes, Some(at))
    }
    fn capture_bounded(
        &self,
        max_bytes: usize,
        deadline: Option<std::time::Instant>,
    ) -> Result<Witness> {
        let mut budget = CaptureBudget {
            remaining: max_bytes,
            deadline,
        };
        budget.take(0, 256)?;
        let mut bindings = Vec::new();
        for [prefix, namespace] in &self.bindings {
            budget.take(prefix.len() + namespace.len(), 80)?;
            bindings.push(Binding {
                prefix: prefix.clone(),
                namespace: namespace.clone(),
            });
        }
        let value = Witness {
            schema: "semantica.source-namespace-state/4".into(),
            bindings,
            default_manager: self.managers[0]
                .as_ref()
                .map(|m| m.capture(&mut budget))
                .transpose()?,
            dataset_manager: self.managers[1]
                .as_ref()
                .map(|m| m.capture(&mut budget))
                .transpose()?,
        };
        if let Some(at) = deadline {
            super::super::source_storage_witness::size_until(&value, max_bytes, at)?;
        } else {
            super::super::source_storage_witness::size(&value, max_bytes)?;
        }
        Ok(value)
    }
}
