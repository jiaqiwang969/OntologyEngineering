//! Exact in-memory ownership transfer, not RDF reconstruction or persistence I/O.
use super::*;
use crate::numeric::SourceHashPolicy;

/// Opaque, Send+Sync state of the existing graph owner. No evaluator session,
/// callbacks, cancellation token or thread-local observer travels with it.
#[derive(Clone)]
pub(crate) struct SourceMemoryState {
    owner: Arc<()>,
    hash_policy: SourceHashPolicy,
    triples: SourceSet<Triple>,
    contexts: Option<SourceGraphContexts>,
    spo: Index,
    pos: Index,
    osp: Index,
    poisoned: bool,
}
impl SourceMemoryGraph {
    pub(crate) fn into_state(self) -> SourceMemoryState {
        SourceMemoryState {
            owner: self.owner,
            hash_policy: self
                .session
                .source_hash()
                .expect("qualified graph hash")
                .policy(),
            triples: self.triples,
            contexts: self.contexts,
            spo: self.spo,
            pos: self.pos,
            osp: self.osp,
            poisoned: self.poisoned,
        }
    }
    pub(crate) fn from_state(
        state: SourceMemoryState,
        session: Rc<QueryNumericSession>,
        cancellation: Option<Arc<AtomicBool>>,
    ) -> Result<Self, SourceGraphError> {
        if session.literal_semantics() != QueryLiteralSemantics::SourceLiteralV1
            || session.context_semantics() != QueryContextSemantics::SourceContextV1
            || session
                .source_hash()
                .is_none_or(|v| v.policy() != state.hash_policy)
        {
            return Err(SourceGraphError::SessionMismatch);
        }
        let graph = Self {
            owner: state.owner,
            session,
            cancellation,
            triples: state.triples,
            contexts: state.contexts,
            spo: state.spo,
            pos: state.pos,
            osp: state.osp,
            poisoned: state.poisoned,
            #[cfg(test)]
            scan_observer: None,
        };
        graph.check_alive()?;
        Ok(graph)
    }
    /// Bound retained spelling and physical index slots, including old parents
    /// and deletion slots. This is a declared state-work budget, not an estimate
    /// of total allocator bytes or an engineering cost/quality score.
    pub(crate) fn retained_work(&self) -> Result<(usize, usize), SourceGraphError> {
        self.check_alive()?;
        let mut seen = std::collections::HashSet::new();
        let mut bytes = 0usize;
        let mut overflow = false;
        let mut visit = |term: &SourceGraphTerm| {
            if seen.insert(Arc::as_ptr(&term.raw)) {
                let mut n = term.raw.source_spelling_len();
                if let LiteralSourceState::Literal(m) = term.raw.literal_source() {
                    if let SourceFact::Known(Some(lang)) = &m.language_original {
                        n = n.saturating_add(lang.len());
                    }
                }
                match bytes.checked_add(n) {
                    Some(v) => bytes = v,
                    None => overflow = true,
                }
            }
        };
        let mut slots = self.triples.physical_slots();
        for triple in self.triples.iter() {
            for term in triple {
                visit(term);
            }
        }
        for index in [&self.spo, &self.pos, &self.osp] {
            slots = slots
                .checked_add(index.entries.len())
                .ok_or(SourceGraphError::IndexOverflow)?;
            for (a, inner) in index.iter() {
                visit(a);
                slots = slots
                    .checked_add(inner.entries.len())
                    .ok_or(SourceGraphError::IndexOverflow)?;
                for (b, leaves) in inner.iter() {
                    visit(b);
                    slots = slots
                        .checked_add(leaves.entries.len())
                        .ok_or(SourceGraphError::IndexOverflow)?;
                    for (c, ()) in leaves.iter() {
                        visit(c);
                    }
                }
            }
        }
        if let Some(contexts) = &self.contexts {
            slots = slots
                .checked_add(contexts.retained_work(&mut visit)?)
                .ok_or(SourceGraphError::IndexOverflow)?;
        }
        if overflow {
            return Err(SourceGraphError::IndexOverflow);
        }
        Ok((slots, bytes))
    }
}
