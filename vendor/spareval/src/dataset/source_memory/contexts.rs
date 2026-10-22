//! Explicit, unquoted source graph contexts in the existing Memory owner.
//! The union fact set, per-context sets, SPO/POS/OSP indexes and memberships
//! retain their own original mutation history; rebuilding from sorted quads
//! is not an equivalent source state. Single-graph admission stays unchanged.
use super::*;

#[derive(Clone)]
pub(super) struct SourceGraphContexts {
    pub(super) default_graph: SourceGraphTerm,
    registered: SourceSet<SourceGraphTerm>,
    graphs: HashMap<SourceTermKey, ContextFacts>,
    memberships: HashMap<[SourceTermKey; 3], Vec<SourceTermKey>>,
}
#[derive(Clone)]
struct ContextFacts {
    graph: SourceGraphTerm,
    triples: SourceSet<Triple>,
}
fn key(triple: &Triple) -> [SourceTermKey; 3] {
    triple.each_ref().map(|v| v.key.clone())
}
fn same_graph(a: &SourceGraphTerm, b: &SourceGraphTerm) -> bool {
    a.raw.same_source_object(&b.raw)
}
fn equal_graph(a: &SourceGraphTerm, b: &SourceGraphTerm) -> Result<bool, SourceGraphError> {
    Ok(a == b)
}

impl SourceGraphContexts {
    pub(super) fn retained_work(&self,visit:&mut impl FnMut(&SourceGraphTerm))->Result<usize,SourceGraphError> {
        visit(&self.default_graph);let mut slots=self.registered.physical_slots();
        for name in self.registered.iter(){visit(name);}
        for facts in self.graphs.values(){
            visit(&facts.graph);slots=slots.checked_add(facts.triples.physical_slots()).ok_or(SourceGraphError::IndexOverflow)?;
            for triple in facts.triples.iter(){for term in triple{visit(term);}}
        }
        for names in self.memberships.values(){slots=slots.checked_add(names.len()).ok_or(SourceGraphError::IndexOverflow)?;}
        Ok(slots)
    }
}
impl SourceMemoryGraph {
    pub(crate) fn new_dataset(
        session: Rc<QueryNumericSession>,
        cancellation: Option<Arc<AtomicBool>>,
        default_graph: Term,
    ) -> Result<Self, SourceGraphError> {
        let mut graph = Self::new(session, cancellation)?;
        if !matches!(default_graph, Term::NamedNode(_) | Term::BlankNode(_)) {
            return Err(SourceGraphError::InvalidTermRole);
        }
        let default_graph = graph.internalize_term(default_graph)?;
        graph.contexts = Some(SourceGraphContexts {
            default_graph,
            registered: SourceSet::new(),
            graphs: HashMap::new(),
            memberships: HashMap::new(),
        });
        Ok(graph)
    }
    pub(crate) fn default_context(&self) -> Result<SourceGraphTerm, SourceGraphError> {
        self.check_alive()?;
        self.contexts.as_ref().map(|v| v.default_graph.clone()).ok_or(SourceGraphError::UnqualifiedGraph)
    }
    pub(super) fn check_context(&self, name: &SourceGraphTerm) -> Result<(), SourceGraphError> {
        self.check_owner(name)?;
        if self.contexts.is_none() {
            return Err(SourceGraphError::UnqualifiedGraph);
        }
        if !matches!(name.raw.rdf(), Ok(Term::NamedNode(_) | Term::BlankNode(_))) {
            return Err(SourceGraphError::InvalidTermRole);
        }
        Ok(())
    }
    pub(super) fn context_triples(
        &self,
        name: Option<&SourceGraphTerm>,
    ) -> Result<Option<&SourceSet<Triple>>, SourceGraphError> {
        self.check_alive()?;
        match name {
            None => Ok(Some(&self.triples)),
            Some(name) => {
                self.check_context(name)?;
                Ok(self
                    .contexts
                    .as_ref()
                    .expect("checked context")
                    .graphs
                    .get(&name.key)
                    .map(|v| &v.triples))
            }
        }
    }
    pub(crate) fn len_context(
        &self,
        name: Option<&SourceGraphTerm>,
    ) -> Result<usize, SourceGraphError> {
        Ok(self.context_triples(name)?.map_or(0, SourceSet::len))
    }
    pub(crate) fn scan_context(
        &self,
        name: Option<&SourceGraphTerm>,
        pattern: [Option<SourceGraphTerm>; 3],
    ) -> Result<Vec<Triple>, SourceGraphError> {
        self.scan_cells_in(pattern.map(|v| v.map(SolutionValue::Rdf)), name)
    }
    pub(crate) fn register_context(
        &mut self,
        name: &SourceGraphTerm,
    ) -> Result<(), SourceGraphError> {
        self.check_context(name)?;
        let hash = self
            .session
            .source_hash()
            .ok_or(SourceGraphError::SessionMismatch)?
            .term_hash(&name.raw)?;
        let result = self
            .contexts
            .as_mut()
            .expect("checked context")
            .registered
            .insert_hashed(
                name.clone(),
                hash,
                &mut same_graph,
                &mut equal_graph,
                &mut || check_cancel(&self.cancellation),
            )
            .map_err(set_error);
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    pub(crate) fn context_names(&self) -> Result<Vec<SourceGraphTerm>, SourceGraphError> {
        self.check_alive()?;
        let contexts = self
            .contexts
            .as_ref()
            .ok_or(SourceGraphError::UnqualifiedGraph)?;
        let mut result = Vec::new();
        result
            .try_reserve(contexts.registered.len())
            .map_err(|_| SourceGraphError::Allocation)?;
        for name in contexts.registered.iter() {
            self.check_alive()?;
            result.push(name.clone());
        }
        Ok(result)
    }
    pub(crate) fn triple_contexts(
        &self,
        triple: &Triple,
    ) -> Result<Vec<SourceGraphTerm>, SourceGraphError> {
        self.triple_hash(triple)?;
        let contexts = self
            .contexts
            .as_ref()
            .ok_or(SourceGraphError::UnqualifiedGraph)?;
        let mut result = Vec::new();
        if let Some(memberships) = contexts.memberships.get(&key(triple)) {
            result
                .try_reserve(memberships.len())
                .map_err(|_| SourceGraphError::Allocation)?;
            for name in memberships {
                self.check_alive()?;
                result.push(
                    contexts
                        .graphs
                        .get(name)
                        .ok_or(SourceGraphError::Poisoned)?
                        .graph
                        .clone(),
                );
            }
        }
        Ok(result)
    }
    pub(crate) fn insert_context(
        &mut self,
        name: &SourceGraphTerm,
        triple: Triple,
    ) -> Result<(), SourceGraphError> {
        self.check_context(name)?;
        let hash = self.triple_hash(&triple)?;
        // Graph.add accepts RDFLib Nodes in all three positions. This explicit
        // source dataset does not borrow the single-graph RDF-role restriction.
        self.register_context(name)?;
        let result = (|| {
            self.insert_fact(triple.clone(), hash)?;
            let contexts = self.contexts.as_mut().expect("checked context");
            contexts
                .graphs
                .try_reserve(1)
                .map_err(|_| SourceGraphError::Allocation)?;
            let facts = contexts
                .graphs
                .entry(name.key.clone())
                .or_insert_with(|| ContextFacts {
                    graph: name.clone(),
                    triples: SourceSet::new(),
                });
            facts.graph = name.clone();
            facts
                .triples
                .insert_hashed(
                    triple.clone(),
                    hash,
                    &mut same_triple_object,
                    &mut equal_triple,
                    &mut || check_cancel(&self.cancellation),
                )
                .map_err(set_error)?;
            contexts
                .memberships
                .try_reserve(1)
                .map_err(|_| SourceGraphError::Allocation)?;
            let names = contexts.memberships.entry(key(&triple)).or_default();
            if !names.contains(&name.key) {
                names
                    .try_reserve(1)
                    .map_err(|_| SourceGraphError::Allocation)?;
                names.push(name.key.clone());
            }
            Ok(())
        })();
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    pub(crate) fn remove_context(
        &mut self,
        name: Option<&SourceGraphTerm>,
        pattern: [Option<SourceGraphTerm>; 3],
    ) -> Result<(), SourceGraphError> {
        // The original store snapshots the selected matching triples, then
        // removes leaf dictionary entries only when the last context is gone.
        let rows = self.scan_context(name, pattern)?;
        let result = (|| {
            for triple in rows {
                let hash = self.triple_hash(&triple)?;
                let mut remove_union = true;
                if let Some(contexts) = &mut self.contexts {
                    let k = key(&triple);
                    let memberships = contexts
                        .memberships
                        .get_mut(&k)
                        .ok_or(SourceGraphError::Poisoned)?;
                    for member in memberships.iter() {
                        if name.is_none_or(|name| name.key == *member) {
                            let facts = contexts
                                .graphs
                                .get_mut(member)
                                .ok_or(SourceGraphError::Poisoned)?;
                            if !facts
                                .triples
                                .discard_hashed(
                                    &triple,
                                    hash,
                                    &mut same_triple_object,
                                    &mut equal_triple,
                                    &mut || check_cancel(&self.cancellation),
                                )
                                .map_err(set_error)?
                            {
                                return Err(SourceGraphError::Poisoned);
                            }
                        }
                    }
                    memberships.retain(|member| name.is_some_and(|name| name.key != *member));
                    remove_union = memberships.is_empty();
                    if remove_union {
                        contexts.memberships.remove(&k);
                    }
                }
                if remove_union {
                    if !self
                        .triples
                        .discard_hashed(
                            &triple,
                            hash,
                            &mut same_triple_object,
                            &mut equal_triple,
                            &mut || check_cancel(&self.cancellation),
                        )
                        .map_err(set_error)?
                    {
                        return Err(SourceGraphError::Poisoned);
                    }
                    let [s, p, o] = &triple;
                    for (index, a, b, c) in [
                        (&mut self.spo, s, p, o),
                        (&mut self.pos, p, o, s),
                        (&mut self.osp, o, s, p),
                    ] {
                        index
                            .get_mut(a)
                            .and_then(|v| v.get_mut(b))
                            .and_then(|v| v.remove(c))
                            .ok_or(SourceGraphError::Poisoned)?;
                    }
                }
            }
            // Targeted empty-context removal discards its set table. Union
            // removal leaves those empty per-context tables in the source store.
            if let Some(name) = name {
                let contexts = self.contexts.as_mut().expect("checked context");
                if contexts
                    .graphs
                    .get(&name.key)
                    .is_some_and(|v| v.triples.len() == 0)
                {
                    contexts.graphs.remove(&name.key);
                }
            }
            Ok(())
        })();
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    pub(crate) fn drop_context(&mut self, name: &SourceGraphTerm) -> Result<(), SourceGraphError> {
        self.check_context(name)?;
        self.remove_context(Some(name), [None, None, None])?;
        let hash = self
            .session
            .source_hash()
            .ok_or(SourceGraphError::SessionMismatch)?
            .term_hash(&name.raw)?;
        let result = self
            .contexts
            .as_mut()
            .expect("checked context")
            .registered
            .discard_hashed(name, hash, &mut same_graph, &mut equal_graph, &mut || {
                check_cancel(&self.cancellation)
            })
            .map_err(set_error);
        if result.is_err() {
            self.poisoned = true;
        }
        result.map(|_| ())
    }
}
