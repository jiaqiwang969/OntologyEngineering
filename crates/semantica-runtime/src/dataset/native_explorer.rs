//! Native-value Explorer on the existing session/cache/revision owner.
use super::*;
use crate::{
    context_native, explorer_mutation::native as state, explorer_search_index::GraphSearchIndex,
    native_values,
};
use semantica_contracts::{
    SemanticExplorerNativeState, SemanticNativeExplorerSearchResult, SemanticNativeValueEnvelope,
};
use semantica_json::{
    native_operations::TextLimits,
    native_value::{NativeLimits, NativeNode as N, NativeValue},
};
use std::time::Instant;
fn timely(d: Instant) -> Result<()> {
    ensure!(Instant::now() < d, "explorer_search_deadline_exceeded");
    Ok(())
}
impl DatasetSession {
    fn native_explorer_limits(&self, mut limits: NativeLimits) -> Result<NativeLimits> {
        let available = self
            .dataset_byte_limit()
            .checked_sub(self.cache_bytes)
            .context("explorer_search_cache_budget")?;
        limits.max_wire_bytes = limits.max_wire_bytes.min(available);
        limits.max_scalar_bytes = limits.max_scalar_bytes.min(available);
        limits.max_nodes = limits.max_nodes.min(available);
        limits.max_links = limits.max_links.min(available);
        Ok(limits)
    }
    fn native_index_wire(
        &self,
        index: &GraphSearchIndex,
        limits: NativeLimits,
        d: Instant,
    ) -> Result<(SemanticNativeValueEnvelope, usize)> {
        let limits = self.native_explorer_limits(limits)?;
        let wire = index.store_native(limits, d)?;
        let bytes = serde_json::to_vec(&wire)?.len();
        ensure!(
            bytes <= limits.max_wire_bytes,
            "explorer_search_cache_budget"
        );
        timely(d)?;
        Ok((wire, bytes))
    }
    fn staged_native_explorer_index(
        &mut self,
        records: &NativeValue,
        context_sha256: &str,
        limits: NativeLimits,
        text: TextLimits,
        d: Instant,
    ) -> Result<ExplorerSearchCache> {
        timely(d)?;
        let digest = self.snapshot()?.dataset_sha256;
        // A cache is bound to the entire current dataset and publication revision.
        // Every publisher invalidates it; only a miss needs to restore saved state.
        let mut cache = match &self.explorer_search_cache {
            Some(cache)
                if cache.native
                    && cache.revision == self.revision
                    && cache.dataset_sha256 == digest =>
            {
                cache.clone()
            }
            _ => {
                let index = if let Some((_, index)) = state::load(self, context_sha256, limits, d)?
                {
                    index
                } else {
                    let nodes = crate::explorer_read::normalize_native_state_nodes(
                        records,
                        state::node_ids(records)?,
                        limits,
                        text,
                        d,
                    )?;
                    let N::List(ids) = &nodes.nodes()[nodes.root()] else {
                        bail!("explorer_native_normalized_nodes")
                    };
                    let mut index = GraphSearchIndex::new_native();
                    index.rebuild_native_original_partial(&nodes, ids, text, d)?;
                    index
                };
                ExplorerSearchCache {
                    native: true,
                    revision: self.revision,
                    dataset_sha256: digest,
                    index,
                    payload_bytes: 0,
                }
            }
        };
        cache.payload_bytes = self.native_index_wire(&cache.index, limits, d)?.1;
        timely(d)?;
        Ok(cache)
    }
    /// Explicit native profile; still one DatasetSession and one derived cache.
    /// Failed construction never publishes a partially initialized ready cache.
    pub fn prepare_native_explorer_index(
        &mut self,
        limits: NativeLimits,
        text: TextLimits,
        d: Instant,
    ) -> Result<()> {
        timely(d)?;
        let limits = self.native_explorer_limits(limits)?;
        let (records, digest) = context_native::read_native_state_value(self, limits, d)?;
        let cache = self.staged_native_explorer_index(&records, &digest, limits, text, d)?;
        timely(d)?;
        self.explorer_search_cache = Some(cache);
        Ok(())
    }
    /// Original GraphSession.search: retrieve matches, then normalize the CURRENT
    /// graph record for each retained ID. This is not the HTTP response serializer.
    pub fn search_native_explorer(
        &mut self,
        input: &NativeValue,
        query: usize,
        limit: usize,
        filters: usize,
        limits: NativeLimits,
        text: TextLimits,
        d: Instant,
    ) -> Result<SemanticNativeExplorerSearchResult> {
        timely(d)?;
        let limits = self.native_explorer_limits(limits)?;
        input.validate(limits)?;
        input.encode(limits)?;
        let (records, digest) = context_native::read_native_state_value(self, limits, d)?;
        let mut cache = self.staged_native_explorer_index(&records, &digest, limits, text, d)?;
        let matches = cache
            .index
            .search_native(input, query, limit, filters, text, d)?;
        let mut by_id = BTreeMap::new();
        for node in state::node_ids(&records)? {
            timely(d)?;
            let N::Text(id) = &records.nodes()[state::field(&records, *node, "id")?] else {
                bail!("explorer_native_node_id")
            };
            by_id.insert(id.as_slice(), *node);
        }
        let mut hits = Vec::new();
        for (id, score) in &matches.matches {
            timely(d)?;
            if let Some(node) = by_id.get(id.as_slice()) {
                hits.push((*node, *score));
            }
        }
        let result = match crate::explorer_read::normalize_native_state_search_results(
            &records, &hits, limits, text, d,
        ) {
            Ok(result) => result,
            Err(error) => {
                // Original search has already updated its LRU before converting the
                // current graph node. Preserve that effect for original conversion errors;
                // host refusals continue to discard the staged cache.
                if crate::explorer_read::is_original_normalization_error(&error) {
                    cache.payload_bytes = self.native_index_wire(&cache.index, limits, d)?.1;
                    timely(d)?;
                    self.explorer_search_cache = Some(cache);
                }
                return Err(error);
            }
        };
        let results = native_values::encode(&result, limits)?;
        let response = SemanticNativeExplorerSearchResult {
            schema: "semantica.explorer-native-search-result/1".into(),
            results,
            cache_hit: matches.cache_hit,
            path: matches.path.into(),
            candidates: matches.candidates.try_into()?,
        };
        ensure!(
            serde_json::to_vec(&response)?.len() <= limits.max_wire_bytes,
            "explorer_native_search_response_budget"
        );
        cache.payload_bytes = self.native_index_wire(&cache.index, limits, d)?.1;
        timely(d)?;
        self.explorer_search_cache = Some(cache);
        Ok(response)
    }
    pub fn observe_native_explorer_index(
        &mut self,
        limits: NativeLimits,
        text: TextLimits,
        d: Instant,
    ) -> Result<SemanticNativeValueEnvelope> {
        timely(d)?;
        let limits = self.native_explorer_limits(limits)?;
        let (records, digest) = context_native::read_native_state_value(self, limits, d)?;
        let cache = self.staged_native_explorer_index(&records, &digest, limits, text, d)?;
        let value = cache.index.observe_native(limits, d)?;
        timely(d)?;
        self.explorer_search_cache = Some(cache);
        Ok(value)
    }
    /// Save through the EXISTING mutation-state graph. GraphSession revision and
    /// history stay separate from the dataset publication revision. This operation
    /// checkpoints already accepted records; it does not implement typed add_*.
    pub fn checkpoint_native_explorer(
        &mut self,
        bridge: bool,
        limits: NativeLimits,
        text: TextLimits,
        d: Instant,
    ) -> Result<SemanticExplorerNativeState> {
        timely(d)?;
        let limits = self.native_explorer_limits(limits)?;
        let (records, digest) = context_native::read_native_state_value(self, limits, d)?;
        let stored = state::load(self, &digest, limits, d)?;
        if let Some((state, _)) = &stored {
            ensure!(
                state.bridge == bridge,
                "explorer_mutation_bridge_mode_changed"
            );
        }
        let mut cache = self.staged_native_explorer_index(&records, &digest, limits, text, d)?;
        let (index, bytes) = self.native_index_wire(&cache.index, limits, d)?;
        cache.payload_bytes = bytes;
        let output = if let Some((mut state, _)) = stored {
            state.index = index;
            state
        } else {
            SemanticExplorerNativeState {
                schema: "semantica.explorer-mutation-state/2".into(),
                context_sha256: digest,
                bridge,
                graph_revision: 0,
                node_type_index: state::initial_types(&records, limits, d)?,
                index,
            }
        };
        let mut staged = self.clone();
        state::save(&mut staged, &output, limits, d)?;
        cache.revision = staged.revision;
        cache.dataset_sha256 = staged.snapshot()?.dataset_sha256;
        // The publisher invalidated derived caches. Restore the just-validated live
        // LRU with its new dataset identity, rather than replacing it with an old copy.
        staged.explorer_search_cache = Some(cache);
        timely(d)?;
        *self = staged;
        Ok(output)
    }
}
