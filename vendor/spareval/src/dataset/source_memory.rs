//! Private source-carrying graph. One fact set and its three indexes
//! retain actual source objects; no RDF-to-provenance cache or graph copy.
//!
//! The original single-graph admission remains distinct from explicit dataset
//! admission. Source context sets retain mutation history and exact objects;
//! imported RDF literals cannot invent source facts.
mod contexts;
mod view;
pub(crate) use view::SourceGraphView;
mod persistent;
pub(crate) use persistent::SourceMemoryState;
use super::{InternalQuad, QueryableDataset};
use crate::eval::source_set::{SourceSet, SourceSetError};
use crate::numeric::{
    LiteralSourceState, QueryContextSemantics, QueryLiteralSemantics, QueryNumericSession,
    RawExpressionValue, RawNumericFault, SourceFact, SourceTermKey, source_identity_key,
};
use crate::source_hash::hash_source_tuple_words;
use crate::tuple::{ErrorValue, SolutionValue};
use contexts::SourceGraphContexts;
use oxrdf::Term;
use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SourceGraphError {
    ForeignOwner,
    SessionMismatch,
    UnknownSourceFacts,
    InvalidTermRole,
    UnqualifiedGraph,
    Cancelled,
    Poisoned,
    Allocation,
    IndexOverflow,
    Raw(RawNumericFault),
}
impl fmt::Display for SourceGraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Raw(error) => write!(f, "{error}"),
            other => write!(f, "source default graph: {other:?}"),
        }
    }
}
impl std::error::Error for SourceGraphError {}
impl From<RawNumericFault> for SourceGraphError {
    fn from(error: RawNumericFault) -> Self {
        Self::Raw(error)
    }
}
fn set_error(error: SourceSetError<SourceGraphError>) -> SourceGraphError {
    match error {
        SourceSetError::Callback(error) => error,
        SourceSetError::Allocation => SourceGraphError::Allocation,
        SourceSetError::IndexOverflow => SourceGraphError::IndexOverflow,
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SourceGraphTerm {
    owner: Arc<()>,
    raw: Arc<RawExpressionValue>,
    key: SourceTermKey,
}
impl PartialEq for SourceGraphTerm {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.owner, &other.owner) && self.key == other.key
    }
}
impl Eq for SourceGraphTerm {}
impl Hash for SourceGraphTerm {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.owner).hash(state);
        self.key.hash(state);
    }
}

/// Index entries retain the first source object at each dictionary level.
/// The hash lookup and insertion-order keys are indexes into the same graph;
/// cached numeric values never participate in dictionary identity.
#[derive(Clone)]
struct SourceDictionary<V> {
    positions: HashMap<SourceGraphLookupKey, usize>,
    entries: Vec<Option<(SourceGraphTerm, V)>>,
}
impl<V> Default for SourceDictionary<V> {
    fn default() -> Self {
        Self {
            positions: HashMap::new(),
            entries: Vec::new(),
        }
    }
}
impl<V> SourceDictionary<V> {
    fn get(&self, term: &SourceGraphTerm) -> Option<&V> {
        self.get_key(&SourceGraphLookupKey::Rdf(term.key.clone()))
    }
    fn get_key(&self, key: &SourceGraphLookupKey) -> Option<&V> {
        self.positions
            .get(key)
            .map(|&at| &self.entries[at].as_ref().expect("live dictionary index").1)
    }
    fn iter(&self) -> impl Iterator<Item = (&SourceGraphTerm, &V)> {
        self.entries
            .iter()
            .flatten()
            .map(|(key, value)| (key, value))
    }
    fn get_mut(&mut self, term: &SourceGraphTerm) -> Option<&mut V> {
        let at = *self
            .positions
            .get(&SourceGraphLookupKey::Rdf(term.key.clone()))?;
        Some(&mut self.entries[at].as_mut().expect("live dictionary index").1)
    }
    fn remove(&mut self, term: &SourceGraphTerm) -> Option<V> {
        let at = self
            .positions
            .remove(&SourceGraphLookupKey::Rdf(term.key.clone()))?;
        self.entries[at].take().map(|(_, value)| value)
    }
}
impl<V: Default> SourceDictionary<V> {
    fn get_or_insert(&mut self, term: &SourceGraphTerm) -> Result<&mut V, SourceGraphError> {
        let key = SourceGraphLookupKey::Rdf(term.key.clone());
        let at = if let Some(&at) = self.positions.get(&key) {
            at
        } else {
            self.positions
                .try_reserve(1)
                .map_err(|_| SourceGraphError::Allocation)?;
            self.entries
                .try_reserve(1)
                .map_err(|_| SourceGraphError::Allocation)?;
            let at = self.entries.len();
            self.entries.push(Some((term.clone(), V::default())));
            self.positions.insert(key, at);
            at
        };
        Ok(&mut self.entries[at].as_mut().expect("live dictionary index").1)
    }
}
type Index = SourceDictionary<SourceDictionary<SourceDictionary<()>>>;
type Triple = [SourceGraphTerm; 3];

/// Returned exceptions are actual dictionary lookup keys, with their native
/// instance identity. Facts contain RDF keys only. A failed lookup determines
/// their empty result; the query compiler never replaces an error with None.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
enum SourceGraphLookupKey {
    Rdf(SourceTermKey),
    ReturnedError(ErrorValue),
}
struct SourceGraphPatternBound {
    key: SourceGraphLookupKey,
    rdf: Option<SourceGraphTerm>,
}
impl SourceGraphPatternBound {
    fn matched_rdf(&self) -> Result<&SourceGraphTerm, SourceGraphError> {
        // Reached only after a matching native index lookup. The RDF-only
        // insertion invariant prevents an exception key from reaching here.
        self.rdf.as_ref().ok_or(SourceGraphError::InvalidTermRole)
    }
}

#[derive(Clone)]
pub(crate) struct SourceMemoryGraph {
    owner: Arc<()>,
    session: Rc<QueryNumericSession>,
    cancellation: Option<Arc<AtomicBool>>,
    triples: SourceSet<Triple>,
    contexts: Option<SourceGraphContexts>,
    spo: Index,
    pos: Index,
    osp: Index,
    poisoned: bool,
    #[cfg(test)]
    scan_observer: Option<Rc<dyn Fn([Option<SolutionValue<RawExpressionValue>>; 3])>>,
}
impl SourceMemoryGraph {
    pub(crate) fn new(
        session: Rc<QueryNumericSession>,
        cancellation: Option<Arc<AtomicBool>>,
    ) -> Result<Self, SourceGraphError> {
        if session.literal_semantics() != QueryLiteralSemantics::SourceLiteralV1
            || session.context_semantics() != QueryContextSemantics::SourceContextV1
            || session.source_hash().is_none()
        {
            return Err(SourceGraphError::SessionMismatch);
        }
        Ok(Self {
            owner: Arc::new(()),
            session,
            cancellation,
            triples: SourceSet::new(),
            contexts: None,
            spo: Index::default(),
            pos: Index::default(),
            osp: Index::default(),
            poisoned: false,
            #[cfg(test)]
            scan_observer: None,
        })
    }
    pub(crate) fn session(&self) -> &Rc<QueryNumericSession> {
        &self.session
    }
    pub(crate) fn cancellation(&self) -> Option<Arc<AtomicBool>> {
        self.cancellation.clone()
    }
    #[cfg(test)]
    pub(crate) fn with_scan_observer(
        mut self,
        observer: Rc<dyn Fn([Option<SolutionValue<RawExpressionValue>>; 3])>,
    ) -> Self {
        self.scan_observer = Some(observer);
        self
    }
    fn check_alive(&self) -> Result<(), SourceGraphError> {
        if self.poisoned {
            return Err(SourceGraphError::Poisoned);
        }
        check_cancel(&self.cancellation)
    }
    fn check_owner(&self, term: &SourceGraphTerm) -> Result<(), SourceGraphError> {
        self.check_alive()?;
        if !Arc::ptr_eq(&self.owner, &term.owner) {
            return Err(SourceGraphError::ForeignOwner);
        }
        Ok(())
    }
    /// An explicit source value is admitted into this graph's owner. This
    /// neither inserts a fact nor canonicalizes an equal object's cache/token.
    pub(crate) fn admit_source(
        &self,
        raw: RawExpressionValue,
    ) -> Result<SourceGraphTerm, SourceGraphError> {
        self.check_alive()?;
        if let LiteralSourceState::Literal(metadata) = raw.literal_source() {
            if matches!(metadata.language_original, SourceFact::Unknown)
                || matches!(metadata.ill_typed, SourceFact::Unknown)
                || matches!(metadata.cached_value_kind, SourceFact::Unknown)
            {
                return Err(SourceGraphError::UnknownSourceFacts);
            }
        }
        self.admit_identity(raw)
    }
    /// Store already observed source keys without claiming an unreceived
    /// converter cache. Only explicit dataset storage admits this boundary;
    /// ordinary graph admission and evaluator transport still require facts.
    pub(crate) fn admit_recorded_source(&self,raw:RawExpressionValue)->Result<SourceGraphTerm,SourceGraphError> {
        self.check_alive()?;
        if self.contexts.is_none(){return Err(SourceGraphError::UnqualifiedGraph);}
        self.admit_identity(raw)
    }
    fn admit_identity(&self,raw:RawExpressionValue)->Result<SourceGraphTerm,SourceGraphError> {
        let key = source_identity_key(&raw)?;
        self.session
            .source_hash()
            .ok_or(SourceGraphError::SessionMismatch)?
            .term_hash(&raw)?;
        Ok(SourceGraphTerm {
            owner: self.owner.clone(),
            raw: Arc::new(raw.ensure_source_object()),
            key,
        })
    }
    /// Actual source-object readback is checked against this graph's owner.
    /// It never resolves source metadata by physical RDF equality.
    pub(crate) fn externalize_source(
        &self,
        term: SourceGraphTerm,
    ) -> Result<RawExpressionValue, SourceGraphError> {
        self.check_owner(&term)?;
        if source_identity_key(&term.raw)? != term.key {
            return Err(SourceGraphError::Raw(RawNumericFault::LossyNumericBackend));
        }
        Ok(term.raw.as_ref().clone())
    }
    pub(crate) fn len(&self) -> usize {
        self.triples.len()
    }
    pub(crate) fn triple_hash(&self, triple: &Triple) -> Result<i64, SourceGraphError> {
        let mut words = [0; 3];
        for (at, term) in triple.iter().enumerate() {
            self.check_owner(term)?;
            words[at] = self
                .session
                .source_hash()
                .ok_or(SourceGraphError::SessionMismatch)?
                .term_hash(&term.raw)?;
        }
        Ok(hash_source_tuple_words(&words))
    }
    pub(crate) fn insert(&mut self, triple: Triple) -> Result<(), SourceGraphError> {
        if let Some(contexts) = &self.contexts {
            return self.insert_context(&contexts.default_graph.clone(), triple);
        }
        let hash = self.triple_hash(&triple)?;
        if !matches!(triple[0].raw.rdf(), Ok(Term::NamedNode(_) | Term::BlankNode(_)))
            || !matches!(triple[1].raw.rdf(), Ok(Term::NamedNode(_)))
            || triple[2].raw.rdf().is_err()
        {
            return Err(SourceGraphError::InvalidTermRole);
        }
        self.insert_fact(triple, hash)
    }
    fn insert_fact(&mut self, triple: Triple, hash: i64) -> Result<(), SourceGraphError> {
        let result = (|| {
            self.triples
                .insert_hashed(
                    triple.clone(),
                    hash,
                    &mut same_triple_object,
                    &mut equal_triple,
                    &mut || check_cancel(&self.cancellation),
                )
                .map_err(set_error)?;
            let [s, p, o] = &triple;
            self.spo
                .get_or_insert(s)?
                .get_or_insert(p)?
                .get_or_insert(o)?;
            self.pos
                .get_or_insert(p)?
                .get_or_insert(o)?
                .get_or_insert(s)?;
            self.osp
                .get_or_insert(o)?
                .get_or_insert(s)?
                .get_or_insert(p)?;
            Ok(())
        })();
        // Set growth can fail after mutation. A failed construction cannot
        // publish its partial graph or resume as a successful query Dataset.
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    fn scan(
        &self,
        subject: Option<&SourceGraphTerm>,
        predicate: Option<&SourceGraphTerm>,
        object: Option<&SourceGraphTerm>,
    ) -> Result<Vec<Triple>, SourceGraphError> {
        self.scan_cells([
            subject.cloned().map(SolutionValue::Rdf),
            predicate.cloned().map(SolutionValue::Rdf),
            object.cloned().map(SolutionValue::Rdf),
        ])
    }
    fn scan_cells(
        &self,
        cells: [Option<SolutionValue<SourceGraphTerm>>; 3],
    ) -> Result<Vec<Triple>, SourceGraphError> {
        self.scan_cells_in(cells, self.contexts.as_ref().map(|contexts| &contexts.default_graph))
    }
    fn scan_cells_in(
        &self,
        cells: [Option<SolutionValue<SourceGraphTerm>>; 3],
        context: Option<&SourceGraphTerm>,
    ) -> Result<Vec<Triple>, SourceGraphError> {
        self.check_alive()?;
        let mut bounds: [Option<SourceGraphPatternBound>; 3] = [None, None, None];
        for (slot, cell) in cells.into_iter().enumerate() {
            bounds[slot] = match cell {
                None | Some(SolutionValue::ExplicitNone) => None,
                Some(SolutionValue::Rdf(term)) => {
                    self.check_owner(&term)?;
                    Some(SourceGraphPatternBound {
                        key: SourceGraphLookupKey::Rdf(term.key.clone()),
                        rdf: Some(term),
                    })
                }
                Some(SolutionValue::ReturnedError(error)) => Some(SourceGraphPatternBound {
                    key: SourceGraphLookupKey::ReturnedError(error),
                    rdf: None,
                }),
            };
        }
        let Some(facts) = self.context_triples(context)? else {
            return Ok(Vec::new());
        };
        let [subject, predicate, object] = &bounds;
        let mut rows = Vec::new();
        let mut push = |triple| -> Result<(), SourceGraphError> {
            self.check_alive()?;
            if context.is_some()
                && !facts
                    .contains_hashed(
                        &triple,
                        self.triple_hash(&triple)?,
                        &mut same_triple_object,
                        &mut equal_triple,
                        &mut || self.check_alive(),
                    )
                    .map_err(set_error)?
            {
                return Ok(());
            }
            rows.try_reserve(1)
                .map_err(|_| SourceGraphError::Allocation)?;
            rows.push(triple);
            Ok(())
        };
        match (subject.as_ref(), predicate.as_ref(), object.as_ref()) {
            (None, None, None) => {
                // Memory.triples iterates contextTriples.copy(), not the live
                // table and not SPO insertion order. Copy reuses stored hashes.
                let snapshot = facts
                    .copy_hashed(&mut same_triple_object, &mut equal_triple, &mut || {
                        self.check_alive()
                    })
                    .map_err(set_error)?;
                for triple in snapshot.iter() {
                    push(triple.clone())?;
                }
            }
            (Some(s), Some(p), Some(o)) => {
                if self
                    .spo
                    .get_key(&s.key)
                    .and_then(|v| v.get_key(&p.key))
                    .and_then(|v| v.get_key(&o.key))
                    .is_some()
                {
                    // Fully bound Graph.triples returns the pattern objects.
                    push([
                        s.matched_rdf()?.clone(),
                        p.matched_rdf()?.clone(),
                        o.matched_rdf()?.clone(),
                    ])?;
                }
            }
            (Some(s), p, o) => {
                if let Some(predicates) = self.spo.get_key(&s.key) {
                    if let Some(p) = p {
                        if let Some(objects) = predicates.get_key(&p.key) {
                            for (o, _) in objects.iter() {
                                push([
                                    s.matched_rdf()?.clone(),
                                    p.matched_rdf()?.clone(),
                                    o.clone(),
                                ])?;
                            }
                        }
                    } else {
                        for (p, objects) in predicates.iter() {
                            if let Some(o) = o {
                                if objects.get_key(&o.key).is_some() {
                                    push([
                                        s.matched_rdf()?.clone(),
                                        p.clone(),
                                        o.matched_rdf()?.clone(),
                                    ])?;
                                }
                            } else {
                                for (o, _) in objects.iter() {
                                    push([s.matched_rdf()?.clone(), p.clone(), o.clone()])?;
                                }
                            }
                        }
                    }
                }
            }
            (None, Some(p), o) => {
                if let Some(objects) = self.pos.get_key(&p.key) {
                    if let Some(o) = o {
                        if let Some(subjects) = objects.get_key(&o.key) {
                            for (s, _) in subjects.iter() {
                                push([
                                    s.clone(),
                                    p.matched_rdf()?.clone(),
                                    o.matched_rdf()?.clone(),
                                ])?;
                            }
                        }
                    } else {
                        for (o, subjects) in objects.iter() {
                            for (s, _) in subjects.iter() {
                                push([s.clone(), p.matched_rdf()?.clone(), o.clone()])?;
                            }
                        }
                    }
                }
            }
            (None, None, Some(o)) => {
                if let Some(subjects) = self.osp.get_key(&o.key) {
                    for (s, predicates) in subjects.iter() {
                        for (p, _) in predicates.iter() {
                            push([s.clone(), p.clone(), o.matched_rdf()?.clone()])?;
                        }
                    }
                }
            }
        }
        Ok(rows)
    }
    pub(crate) fn source_quads_for_cells<'a>(
        &self,
        cells: [Option<SolutionValue<SourceGraphTerm>>; 3],
    ) -> Box<dyn Iterator<Item = Result<InternalQuad<SourceGraphTerm>, SourceGraphError>> + 'a>
    {
        #[cfg(test)]
        if let Some(observer) = &self.scan_observer {
            observer(cells.each_ref().map(|v| {
                v.as_ref().map(|v| match v {
                    SolutionValue::Rdf(v) => SolutionValue::Rdf(v.raw.as_ref().clone()),
                    SolutionValue::ExplicitNone => SolutionValue::ExplicitNone,
                    SolutionValue::ReturnedError(v) => SolutionValue::ReturnedError(v.clone()),
                })
            }));
        }
        match self.scan_cells(cells) {
            Ok(rows) => Box::new(SourceTripleIter {
                rows: rows.into_iter(),
                cancellation: self.cancellation.clone(),
                terminal: false,
            }),
            Err(error) => Box::new(std::iter::once(Err(error))),
        }
    }
}
fn check_cancel(cancellation: &Option<Arc<AtomicBool>>) -> Result<(), SourceGraphError> {
    if cancellation
        .as_ref()
        .is_some_and(|flag| flag.load(Ordering::Relaxed))
    {
        Err(SourceGraphError::Cancelled)
    } else {
        Ok(())
    }
}
fn same_triple_object(left: &Triple, right: &Triple) -> bool {
    left.iter()
        .zip(right)
        .all(|(l, r)| l.raw.same_source_object(&r.raw))
}
fn equal_triple(left: &Triple, right: &Triple) -> Result<bool, SourceGraphError> {
    Ok(left == right)
}

struct SourceTripleIter {
    rows: std::vec::IntoIter<Triple>,
    cancellation: Option<Arc<AtomicBool>>,
    terminal: bool,
}
impl Iterator for SourceTripleIter {
    type Item = Result<InternalQuad<SourceGraphTerm>, SourceGraphError>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.terminal {
            return None;
        }
        if let Err(error) = check_cancel(&self.cancellation) {
            self.terminal = true;
            return Some(Err(error));
        }
        match self.rows.next() {
            Some([subject, predicate, object]) => Some(Ok(InternalQuad {
                subject,
                predicate,
                object,
                graph_name: None,
            })),
            None => {
                self.terminal = true;
                None
            }
        }
    }
}

impl<'a> QueryableDataset<'a> for SourceMemoryGraph {
    type InternalTerm = SourceGraphTerm;
    type Error = SourceGraphError;
    fn internal_quads_for_pattern(
        &self,
        subject: Option<&SourceGraphTerm>,
        predicate: Option<&SourceGraphTerm>,
        object: Option<&SourceGraphTerm>,
        graph_name: Option<Option<&SourceGraphTerm>>,
    ) -> impl Iterator<Item = Result<InternalQuad<SourceGraphTerm>, SourceGraphError>> + use<'a>
    {
        let result = if graph_name == Some(None) {
            self.scan(subject, predicate, object)
        } else {
            Err(SourceGraphError::UnqualifiedGraph)
        };
        let iter: Box<
            dyn Iterator<Item = Result<InternalQuad<SourceGraphTerm>, SourceGraphError>> + 'a,
        > = match result {
            Ok(rows) => Box::new(SourceTripleIter {
                rows: rows.into_iter(),
                cancellation: self.cancellation.clone(),
                terminal: false,
            }),
            Err(error) => Box::new(std::iter::once(Err(error))),
        };
        iter
    }
    fn internal_named_graphs(
        &self,
    ) -> impl Iterator<Item = Result<SourceGraphTerm, SourceGraphError>> + use<'a> {
        std::iter::once(Err(SourceGraphError::UnqualifiedGraph))
    }
    fn contains_internal_graph_name(&self, _: &SourceGraphTerm) -> Result<bool, SourceGraphError> {
        Err(SourceGraphError::UnqualifiedGraph)
    }
    fn internalize_term(&self, term: Term) -> Result<SourceGraphTerm, SourceGraphError> {
        // Nonliterals have complete identity facts. An imported RDF literal
        // deliberately fails, including physically collapsed plain/xsd:string.
        self.admit_source(RawExpressionValue::from_import(term))
    }
    fn externalize_term(&self, term: SourceGraphTerm) -> Result<Term, SourceGraphError> {
        Ok(self.externalize_source(term)?.rdf()?.clone())
    }
}
