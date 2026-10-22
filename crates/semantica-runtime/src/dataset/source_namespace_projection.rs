//! Local checking projection only; never registered as an executed Dataset.
use super::*;
use semantica_contracts::{SemanticSourceNamespaceManager4, SemanticSourceNamespaceState4};
use std::time::Instant;

impl NamespaceState {
    pub(in crate::dataset) fn retained_charge(&self) -> (usize, usize) {
        (self.charged_bytes, self.charged_entries)
    }
    pub(in crate::dataset) fn checking_projection(
        witness: &SemanticSourceNamespaceState4,
        charged_bytes: usize,
        charged_entries: usize,
        limit: usize,
        at: Instant,
    ) -> Result<Self> {
        super::super::source_namespace_witness::validate_namespace_witness_until(
            witness, limit, at,
        )?;
        let manager = |w: &SemanticSourceNamespaceManager4| -> Result<Manager> {
            let mut trie = vec![TrieNode::default(); w.nodes.len()];
            for (i, n) in w.nodes.iter().enumerate() {
                ensure!(Instant::now() < at, "source_turtle_deadline");
                for edge in &n.children {
                    let target = usize::try_from(edge.target)?;
                    trie[target].value = edge.namespace.clone();
                    trie[i].children.push(target);
                }
            }
            Ok(Manager {
                trie,
                strie: w
                    .split_cache
                    .iter()
                    .map(|s| Ok((s.namespace.clone(), usize::try_from(s.target)?)))
                    .collect::<Result<_>>()?,
                cache: w
                    .qname_cache
                    .iter()
                    .map(|q| {
                        (
                            q.uri.clone(),
                            [q.prefix.clone(), q.namespace.clone(), q.local.clone()],
                        )
                    })
                    .collect(),
            })
        };
        Ok(Self {
            bindings: witness
                .bindings
                .iter()
                .map(|b| [b.prefix.clone(), b.namespace.clone()])
                .collect(),
            managers: [
                witness.default_manager.as_ref().map(manager).transpose()?,
                witness.dataset_manager.as_ref().map(manager).transpose()?,
            ],
            charged_bytes,
            charged_entries,
        })
    }
}
