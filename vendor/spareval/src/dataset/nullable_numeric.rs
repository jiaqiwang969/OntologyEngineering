//! Private value carrier over the same underlying queryable dataset.
//! No graph copy, extra store, or RDF-identity-to-cache map is constructed.
use super::{InternalQuad, QueryableDataset};
use crate::QueryEvaluationError;
use crate::numeric::{
    LiteralSourceState, NumericCacheState, QueryLiteralSemantics, QueryNumericSession,
    RawExpressionValue, RawNumericFault, SourceDatatype, SourceFact, SourceTermKey,
    source_identity_key,
};
use crate::tuple::SolutionValue;
use oxrdf::Term;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::sync::{Arc, atomic::AtomicBool};

#[derive(Clone, Debug)]
pub(crate) struct CachedInternalTerm<T> {
    rdf: T,
    numeric: NumericCacheState,
    // The evaluated source object, never indexed by RDF equality. Scans do not
    // invent the source facts discarded by their backend representation.
    source_raw: Option<Rc<RawExpressionValue>>,
}
impl<T> CachedInternalTerm<T> {
    pub(crate) fn rdf(&self) -> &T {
        &self.rdf
    }
    pub(crate) fn numeric(&self) -> &NumericCacheState {
        &self.numeric
    }
    pub(crate) fn literal_source(&self) -> Option<&LiteralSourceState> {
        self.source_raw
            .as_deref()
            .map(RawExpressionValue::literal_source)
    }
    pub(crate) fn source_identity_key(&self) -> Result<SourceTermKey, RawNumericFault> {
        source_identity_key(
            self.source_raw
                .as_deref()
                .ok_or(RawNumericFault::UnknownSourceLiteralIdentity)?,
        )
    }
    /// Exact source object identity survives codec crossings. It grants no
    /// missing source facts and never bypasses DISTINCT/group hashing.
    pub(crate) fn same_transport_instance(&self, other: &Self) -> bool {
        match (&self.source_raw, &other.source_raw) {
            (Some(left), Some(right)) => left.same_source_object(right),
            _ => false,
        }
    }
    pub(crate) fn source_context_equal(&self, other: &Self) -> Result<bool, RawNumericFault> {
        if self.same_transport_instance(other) {
            return Ok(true);
        }
        crate::numeric::source_context_equal(self.source_raw()?, other.source_raw()?)
    }
    pub(crate) fn source_raw(&self) -> Result<&RawExpressionValue, RawNumericFault> {
        self.source_raw
            .as_deref()
            .ok_or(RawNumericFault::UnknownSourceLiteralIdentity)
    }
    fn unknown(rdf: T) -> Self {
        Self {
            rdf,
            numeric: NumericCacheState::UnknownNormalized,
            source_raw: None,
        }
    }
}
impl<T: PartialEq> PartialEq for CachedInternalTerm<T> {
    fn eq(&self, other: &Self) -> bool {
        self.rdf == other.rdf
    }
}
impl<T: Eq> Eq for CachedInternalTerm<T> {}
impl<T: Hash> Hash for CachedInternalTerm<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.rdf.hash(state);
    }
}

mod source_sealed {
    pub trait Sealed {}
}
/// A private admission contract; only the native source-carrying backend is
/// sealed in here. SourceLiteralV1 alone cannot grant graph provenance.
pub(crate) trait SourceQueryableDataset<'a>:
    QueryableDataset<'a> + source_sealed::Sealed
{
    fn source_session(&self) -> &Rc<QueryNumericSession>;
    fn has_source_contexts(&self) -> bool { false }
    fn source_scoped_graph(&self, _: Self::InternalTerm) -> Result<Option<Self>, Self::Error> where Self:Sized { Ok(None) }
    fn source_cancellation(&self) -> Option<Arc<AtomicBool>>;
    fn internalize_source(
        &self,
        raw: RawExpressionValue,
    ) -> Result<Self::InternalTerm, Self::Error>;
    fn externalize_source(
        &self,
        term: Self::InternalTerm,
    ) -> Result<RawExpressionValue, Self::Error>;
    fn source_quads_for_cells(
        &self,
        cells: [Option<SolutionValue<Self::InternalTerm>>; 3],
    ) -> Box<dyn Iterator<Item = Result<InternalQuad<Self::InternalTerm>, Self::Error>> + 'a>;
}
impl source_sealed::Sealed for super::source_memory::SourceMemoryGraph {}
impl<'a> SourceQueryableDataset<'a> for super::source_memory::SourceMemoryGraph {
    fn source_session(&self) -> &Rc<QueryNumericSession> {
        self.session()
    }
    fn source_cancellation(&self) -> Option<Arc<AtomicBool>> {
        self.cancellation()
    }
    fn internalize_source(
        &self,
        raw: RawExpressionValue,
    ) -> Result<Self::InternalTerm, Self::Error> {
        self.admit_source(raw)
    }
    fn externalize_source(
        &self,
        term: Self::InternalTerm,
    ) -> Result<RawExpressionValue, Self::Error> {
        super::source_memory::SourceMemoryGraph::externalize_source(self, term)
    }
    fn source_quads_for_cells(
        &self,
        cells: [Option<SolutionValue<Self::InternalTerm>>; 3],
    ) -> Box<dyn Iterator<Item = Result<InternalQuad<Self::InternalTerm>, Self::Error>> + 'a> {
        super::source_memory::SourceMemoryGraph::source_quads_for_cells(self, cells)
    }
}
impl source_sealed::Sealed for super::source_memory::SourceGraphView {}
impl<'a> SourceQueryableDataset<'a> for super::source_memory::SourceGraphView {
    fn source_session(&self)->&Rc<QueryNumericSession> {self.session()}
    fn source_cancellation(&self)->Option<Arc<AtomicBool>> {self.cancellation()}
    fn has_source_contexts(&self)->bool {true}
    fn source_scoped_graph(&self,name:Self::InternalTerm)->Result<Option<Self>,Self::Error> {self.scoped(name).map(Some)}
    fn internalize_source(&self,raw:RawExpressionValue)->Result<Self::InternalTerm,Self::Error> {self.admit_source(raw)}
    fn externalize_source(&self,term:Self::InternalTerm)->Result<RawExpressionValue,Self::Error> {super::source_memory::SourceGraphView::externalize_source(self,term)}
    fn source_quads_for_cells(&self,cells:[Option<SolutionValue<Self::InternalTerm>>;3])->Box<dyn Iterator<Item=Result<InternalQuad<Self::InternalTerm>,Self::Error>>+'a> {super::source_memory::SourceGraphView::source_quads_for_cells(self,cells)}
}
struct SourceDatasetTransport<'a, D: QueryableDataset<'a>> {
    session: Rc<QueryNumericSession>,
    cancellation: Option<Arc<AtomicBool>>,
    contexts: bool,
    scope: fn(&D, D::InternalTerm) -> Result<Option<D>, D::Error>,
    write: fn(&D, RawExpressionValue) -> Result<D::InternalTerm, D::Error>,
    read: fn(&D, D::InternalTerm) -> Result<RawExpressionValue, D::Error>,
    scan: fn(
        &D,
        [Option<SolutionValue<D::InternalTerm>>; 3],
    ) -> Box<dyn Iterator<Item = Result<InternalQuad<D::InternalTerm>, D::Error>> + 'a>,
    marker: std::marker::PhantomData<&'a ()>,
}
impl<'a, D: QueryableDataset<'a>> Clone for SourceDatasetTransport<'a, D> {
    fn clone(&self) -> Self {
        Self {
            session: self.session.clone(),
            cancellation: self.cancellation.clone(),
            contexts: self.contexts,
            scope: self.scope,
            write: self.write,
            read: self.read,
            scan: self.scan,
            marker: std::marker::PhantomData,
        }
    }
}
pub(crate) struct NumericDataset<'a, D: QueryableDataset<'a>> {
    inner: Rc<D>,
    literal_semantics: QueryLiteralSemantics,
    source: Option<SourceDatasetTransport<'a, D>>,
}
impl<'a, D: QueryableDataset<'a>> Clone for NumericDataset<'a, D> {
    fn clone(&self) -> Self {
        Self {
            inner: Rc::clone(&self.inner),
            literal_semantics: self.literal_semantics,
            source: self.source.clone(),
        }
    }
}
impl<'a, D: QueryableDataset<'a>> NumericDataset<'a, D> {
    pub(crate) fn new(inner: D) -> Self {
        Self::new_with_literal_semantics(inner, QueryLiteralSemantics::RdfIdentityV1)
    }
    pub(crate) fn new_with_literal_semantics(
        inner: D,
        literal_semantics: QueryLiteralSemantics,
    ) -> Self {
        Self {
            inner: Rc::new(inner),
            literal_semantics,
            source: None,
        }
    }
    pub(crate) fn new_with_source_dataset(
        inner: D,
        session: Rc<QueryNumericSession>,
    ) -> Result<Self, QueryEvaluationError>
    where
        D: SourceQueryableDataset<'a>,
    {
        if !Rc::ptr_eq(inner.source_session(), &session)
            || session.literal_semantics() != QueryLiteralSemantics::SourceLiteralV1
        {
            return Err(QueryEvaluationError::Unexpected(Box::new(
                RawNumericFault::InvalidSourceOrigin,
            )));
        }
        Ok(Self {
            source: Some(SourceDatasetTransport {
                cancellation: inner.source_cancellation(),
                session,
                contexts: inner.has_source_contexts(),
                scope: D::source_scoped_graph,
                write: D::internalize_source,
                read: D::externalize_source,
                scan: D::source_quads_for_cells,
                marker: std::marker::PhantomData,
            }),
            inner: Rc::new(inner),
            literal_semantics: QueryLiteralSemantics::SourceLiteralV1,
        })
    }
    pub(crate) fn has_source_contexts(&self)->bool {self.source.as_ref().is_some_and(|v|v.contexts)}
    pub(crate) fn scoped_source_graph(&self,name:CachedInternalTerm<D::InternalTerm>)->Result<Option<Self>,QueryEvaluationError> {
        let Some(source)=&self.source else {return Ok(None)};
        let inner=(source.scope)(&self.inner,name.rdf).map_err(|e|QueryEvaluationError::Dataset(Box::new(e)))?;
        Ok(inner.map(|inner|Self{inner:Rc::new(inner),literal_semantics:self.literal_semantics,source:self.source.clone()}))
    }
    pub(crate) fn source_cancellation(&self) -> Option<Arc<AtomicBool>> {
        self.source
            .as_ref()
            .and_then(|source| source.cancellation.clone())
    }
    pub(crate) fn has_source_graph_session(&self, session: &Rc<QueryNumericSession>) -> bool {
        self.source
            .as_ref()
            .is_some_and(|source| Rc::ptr_eq(&source.session, session))
    }
    pub(crate) fn source_triples_for_cells(
        &self,
        cells: [Option<SolutionValue<CachedInternalTerm<D::InternalTerm>>>; 3],
    ) -> Result<
        Box<
            dyn Iterator<
                    Item = Result<[CachedInternalTerm<D::InternalTerm>; 3], QueryEvaluationError>,
                > + 'a,
        >,
        QueryEvaluationError,
    > {
        let source = self.source.as_ref().cloned().ok_or_else(|| {
            QueryEvaluationError::Unexpected(Box::new(RawNumericFault::InvalidSourceOrigin))
        })?;
        let cells = cells.map(|cell| {
            cell.map(|cell| match cell {
                SolutionValue::Rdf(value) => SolutionValue::Rdf(value.rdf),
                SolutionValue::ExplicitNone => SolutionValue::ExplicitNone,
                SolutionValue::ReturnedError(error) => SolutionValue::ReturnedError(error),
            })
        });
        let inner = self.inner.clone();
        let rows = (source.scan)(&inner, cells);
        Ok(Box::new(rows.map(move |quad| {
            let quad = quad.map_err(|error| QueryEvaluationError::Dataset(Box::new(error)))?;
            let wrap = |rdf: D::InternalTerm| -> Result<_, QueryEvaluationError> {
                let raw = (source.read)(&inner, rdf.clone())
                    .map_err(|error| QueryEvaluationError::Dataset(Box::new(error)))?;
                Ok(CachedInternalTerm {
                    rdf,
                    numeric: raw.numeric().clone(),
                    source_raw: Some(Rc::new(raw)),
                })
            };
            Ok([
                wrap(quad.subject)?,
                wrap(quad.predicate)?,
                wrap(quad.object)?,
            ])
        })))
    }

    /// Native source paths use these exact scan/read/write owners. An RDF-only
    /// dataset or a foreign preparation/session cannot admit the compiler.
    pub(crate) fn compile_source_path(
        &self,
        definition: &spargebra::SourcePathDefinition,
        registry: Rc<crate::numeric::PreparedSourceLiteralRegistry>,
    ) -> Result<crate::eval::source_path::CompiledSourcePath<'a, CachedInternalTerm<D::InternalTerm>>, QueryEvaluationError> {
        if !self.has_source_graph_session(registry.session()) {
            return Err(QueryEvaluationError::Unexpected(Box::new(RawNumericFault::InvalidSourceOrigin)));
        }
        let scan_owner = self.clone();
        let read_owner = self.clone();
        let write_owner = self.clone();
        let runtime = Rc::new(crate::eval::source_path::SourcePathRuntime {
            scan: Rc::new(move |cells| scan_owner.source_triples_for_cells(cells)),
            read: Rc::new(move |value| read_owner.externalize_raw(value)),
            write: Rc::new(move |raw| write_owner.internalize_raw(raw)),
            session: registry.session().clone(),
            registry,
            token: crate::eval::source_path::graph_cancellation(self.source_cancellation()),
        });
        crate::eval::source_path::compile_source_path(runtime, definition)
    }
    fn requires_source_readback(&self, raw: &RawExpressionValue) -> bool {
        source_facts_are_known(raw)
            || (self.literal_semantics == QueryLiteralSemantics::SourceLiteralV1
                && matches!(raw.literal_source(), LiteralSourceState::NotLiteral))
    }
}
impl<'a, D: QueryableDataset<'a>> NumericDataset<'a, D> {
    pub(crate) fn internalize_raw(
        &self,
        raw: RawExpressionValue,
    ) -> Result<CachedInternalTerm<D::InternalTerm>, QueryEvaluationError> {
        let raw = if self.source.is_some() {
            raw.ensure_source_object()
        } else {
            raw
        };
        let numeric = raw.numeric().clone();
        let source_known = self.requires_source_readback(&raw);
        let rdf = if let Some(source) = &self.source {
            (source.write)(&self.inner, raw.clone())
                .map_err(|error| QueryEvaluationError::Dataset(Box::new(error)))?
        } else {
            self.inner.internalize_term(raw.rdf().map_err(raw_fault)?.clone())
                .map_err(|error| QueryEvaluationError::Dataset(Box::new(error)))?
        };
        if source_known || matches!(&numeric, NumericCacheState::Known(_)) {
            if let Some(source) = &self.source {
                let actual = (source.read)(&self.inner, rdf.clone())
                    .map_err(|error| QueryEvaluationError::Dataset(Box::new(error)))?;
                if actual != raw || !actual.same_source_object(&raw)
                    || actual.literal_source() != raw.literal_source()
                {
                    return Err(raw_fault(RawNumericFault::LossyNumericBackend));
                }
            }
            if let Ok(expected) = raw.rdf() {
                let actual = self.inner.externalize_term(rdf.clone())
                    .map_err(|error| QueryEvaluationError::Dataset(Box::new(error)))?;
                if &actual != expected {
                    return Err(raw_fault(RawNumericFault::LossyNumericBackend));
                }
            } else if self.source.is_none() {
                return Err(raw_fault(RawNumericFault::SourceTermNotRdf("Variable")));
            }
        }
        Ok(CachedInternalTerm {
            rdf,
            numeric,
            source_raw: Some(Rc::new(raw.ensure_source_object())),
        })
    }
    pub(crate) fn externalize_raw(
        &self,
        value: CachedInternalTerm<D::InternalTerm>,
    ) -> Result<RawExpressionValue, QueryEvaluationError> {
        if let Some(source) = &self.source {
            // Read the sealed source owner first. The cached copy cannot hide
            // a failed backend read, changed key, value, or source identity.
            let actual = (source.read)(&self.inner, value.rdf.clone())
                .map_err(|error| QueryEvaluationError::Dataset(Box::new(error)))?;
            if value.source_raw.as_deref().is_some_and(|original| {
                actual != *original || !actual.same_source_object(original)
                    || actual.literal_source() != original.literal_source()
            }) {
                return Err(raw_fault(RawNumericFault::LossyNumericBackend));
            }
            if let Ok(expected) = actual.rdf() {
                let rdf = self.inner.externalize_term(value.rdf)
                    .map_err(|error| QueryEvaluationError::Dataset(Box::new(error)))?;
                if &rdf != expected { return Err(raw_fault(RawNumericFault::LossyNumericBackend)); }
            }
            return Ok(actual);
        }
        let rdf = self.inner.externalize_term(value.rdf)
            .map_err(|error| QueryEvaluationError::Dataset(Box::new(error)))?;
        if let Some(original) = value.source_raw.as_deref() {
            if self.requires_source_readback(original)
                || self.literal_semantics == QueryLiteralSemantics::SourceLiteralV1
            {
                // Real externalization happens first: a cached source snapshot
                // cannot hide a backend error or a changed RDF representation.
                if &rdf != original.rdf().map_err(raw_fault)? {
                    return Err(QueryEvaluationError::Unexpected(Box::new(
                        RawNumericFault::LossyNumericBackend,
                    )));
                }
                return Ok(original.clone());
            }
        }
        Ok(match value.numeric {
            // A scan can carry an opaque nonliteral with this conservative
            // state; classify only after real externalization, without parsing.
            NumericCacheState::UnknownNormalized => RawExpressionValue::from_import(rdf),
            numeric => RawExpressionValue::from_parts(rdf, numeric),
        })
    }
}
fn raw_fault(error: RawNumericFault) -> QueryEvaluationError { QueryEvaluationError::Unexpected(Box::new(error)) }
// A partially declared source fact is still a promise that must survive the
// backend. Unknown imports retain the preexisting RDF/cache path without an
// extra conversion or a guessed plain/xsd:string distinction.
fn source_facts_are_known(raw: &RawExpressionValue) -> bool {
    let LiteralSourceState::Literal(metadata) = raw.literal_source() else {
        return false;
    };
    !matches!(&metadata.datatype, SourceDatatype::Unknown)
        || !matches!(&metadata.language_original, SourceFact::Unknown)
        || !matches!(&metadata.ill_typed, SourceFact::Unknown)
        || !matches!(&metadata.cached_value_kind, SourceFact::Unknown)
}

impl<'a, D: QueryableDataset<'a>> QueryableDataset<'a> for NumericDataset<'a, D> {
    type InternalTerm = CachedInternalTerm<D::InternalTerm>;
    type Error = D::Error;

    fn internal_quads_for_pattern(
        &self,
        subject: Option<&Self::InternalTerm>,
        predicate: Option<&Self::InternalTerm>,
        object: Option<&Self::InternalTerm>,
        graph_name: Option<Option<&Self::InternalTerm>>,
    ) -> impl Iterator<Item = Result<InternalQuad<Self::InternalTerm>, Self::Error>> + use<'a, D>
    {
        let source = self.source.clone();
        let inner = self.inner.clone();
        self.inner
            .internal_quads_for_pattern(
                subject.map(CachedInternalTerm::rdf),
                predicate.map(CachedInternalTerm::rdf),
                object.map(CachedInternalTerm::rdf),
                graph_name.map(|g| g.map(CachedInternalTerm::rdf)),
            )
            .map(move |quad| {
                let q = quad?;
                let wrap = |rdf: D::InternalTerm| -> Result<_, D::Error> {
                    if let Some(source) = &source {
                        let raw = (source.read)(&inner, rdf.clone())?;
                        Ok(CachedInternalTerm {
                            rdf,
                            numeric: raw.numeric().clone(),
                            source_raw: Some(Rc::new(raw)),
                        })
                    } else {
                        Ok(CachedInternalTerm::unknown(rdf))
                    }
                };
                Ok(InternalQuad {
                    subject: wrap(q.subject)?,
                    predicate: wrap(q.predicate)?,
                    object: wrap(q.object)?,
                    graph_name: q.graph_name.map(wrap).transpose()?,
                })
            })
    }
    fn internal_named_graphs(
        &self,
    ) -> impl Iterator<Item = Result<Self::InternalTerm, Self::Error>> + use<'a, D> {
        let source = self.source.clone();
        let inner = self.inner.clone();
        self.inner.internal_named_graphs().map(move |value| {
            let rdf = value?;
            if let Some(source) = &source {
                // Graph bindings enter the same source joins and hash sets as
                // triple bindings. Read the sealed owner; native identity alone
                // cannot supply source provenance, nor can a cache hide failure.
                let raw = (source.read)(&inner, rdf.clone())?;
                Ok(CachedInternalTerm {
                    rdf,
                    numeric: raw.numeric().clone(),
                    source_raw: Some(Rc::new(raw)),
                })
            } else {
                Ok(CachedInternalTerm::unknown(rdf))
            }
        })
    }
    fn contains_internal_graph_name(
        &self,
        graph_name: &Self::InternalTerm,
    ) -> Result<bool, Self::Error> {
        self.inner.contains_internal_graph_name(graph_name.rdf())
    }
    fn internalize_term(&self, term: Term) -> Result<Self::InternalTerm, Self::Error> {
        let raw = RawExpressionValue::from_import(term.clone());
        let numeric = raw.numeric().clone();
        // This generic trait cannot return our typed integrity fault. Only
        // internalize_raw can grant a checked source snapshot in the new lane.
        // Preserve the old lane's representation and conversion schedule.
        let source_raw =
            (self.literal_semantics == QueryLiteralSemantics::RdfIdentityV1).then(|| Rc::new(raw));
        self.inner
            .internalize_term(term)
            .map(|rdf| CachedInternalTerm {
                rdf,
                numeric,
                source_raw,
            })
    }
    fn externalize_term(&self, term: Self::InternalTerm) -> Result<Term, Self::Error> {
        self.inner.externalize_term(term.rdf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::numeric::{NumericInputOrigin, QueryNumericSession};
    use oxrdf::{Dataset, Literal, NamedNode};
    use semantica_numeric::{DecimalContext, NumericLimits};
    use std::collections::hash_map::DefaultHasher;

    #[test]
    fn same_dataset_round_trip_keeps_each_cache_and_original_hash_bytes() {
        let graph = Dataset::new();
        let dataset = NumericDataset::new(&graph);
        let session = QueryNumericSession::new(
            DecimalContext::default(),
            NumericLimits::default(),
            None,
            true,
        )
        .unwrap();
        let datatype = NamedNode::new("http://www.w3.org/2001/XMLSchema#decimal").unwrap();
        let a = session
            .construct_literal(
                &Literal::new_typed_literal("1E+2", datatype.clone()),
                NumericInputOrigin::DirectString { normalize: true },
            )
            .unwrap();
        let b = session
            .construct_literal(
                &Literal::new_typed_literal("100", datatype),
                NumericInputOrigin::DirectString { normalize: true },
            )
            .unwrap();
        let a = dataset.internalize_raw(a).unwrap();
        let b = dataset.internalize_raw(b).unwrap();
        assert!(a == b);
        let mut wrapped = DefaultHasher::new();
        let mut original = DefaultHasher::new();
        a.hash(&mut wrapped);
        a.rdf().hash(&mut original);
        assert_eq!(wrapped.finish(), original.finish());
        let NumericCacheState::Known(ac) = a.numeric() else {
            panic!("missing first cache");
        };
        let NumericCacheState::Known(bc) = b.numeric() else {
            panic!("missing second cache");
        };
        assert_ne!(ac.value(), bc.value());
        let out = dataset.externalize_raw(a.clone()).unwrap();
        let NumericCacheState::Known(cache) = out.numeric() else {
            panic!("lost cache");
        };
        assert_eq!(cache.value(), ac.value());
        assert!(graph.is_empty());
    }
}

#[cfg(test)]
mod source_transport_tests {
    use super::*;
    use crate::numeric::{
        QueryGroupSemantics, QueryLiteralSemantics, QueryNumericCapabilities, QueryNumericSession,
    };
    use oxrdf::{Literal, NamedNode};
    use semantica_numeric::{DecimalContext, NumericLimits};
    use std::cell::Cell;
    use std::fmt;
    use std::sync::Arc;

    #[derive(Clone, Debug)]
    struct ConversionError(Arc<()>);
    impl fmt::Display for ConversionError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("source transport conversion")
        }
    }
    impl std::error::Error for ConversionError {}

    struct DatasetControl {
        internalizations: Rc<Cell<usize>>,
        replacement: Term,
        externalizations: Rc<Cell<usize>>,
        loss_at: Rc<Cell<usize>>,
        fail_at: Rc<Cell<usize>>,
        error: ConversionError,
    }
    impl DatasetControl {
        fn new() -> Self {
            Self {
                internalizations: Rc::new(Cell::new(0)),
                replacement: Literal::new_simple_literal("backend replacement").into(),
                externalizations: Rc::new(Cell::new(0)),
                loss_at: Rc::new(Cell::new(0)),
                fail_at: Rc::new(Cell::new(0)),
                error: ConversionError(Arc::new(())),
            }
        }
    }
    impl<'a> QueryableDataset<'a> for DatasetControl {
        type InternalTerm = Term;
        type Error = ConversionError;
        fn internal_quads_for_pattern(
            &self,
            _: Option<&Term>,
            _: Option<&Term>,
            _: Option<&Term>,
            _: Option<Option<&Term>>,
        ) -> impl Iterator<Item = Result<InternalQuad<Term>, ConversionError>> + use<'a> {
            std::iter::empty()
        }
        fn internalize_term(&self, term: Term) -> Result<Term, ConversionError> {
            self.internalizations.set(self.internalizations.get() + 1);
            Ok(term)
        }
        fn externalize_term(&self, term: Term) -> Result<Term, ConversionError> {
            let call = self.externalizations.get() + 1;
            self.externalizations.set(call);
            if call == self.fail_at.get() {
                return Err(self.error.clone());
            }
            if call == self.loss_at.get() {
                return Ok(self.replacement.clone());
            }
            Ok(term)
        }
    }
    fn session() -> QueryNumericSession {
        QueryNumericSession::new_with_literal_semantics(
            DecimalContext::default(),
            NumericLimits::default(),
            None,
            true,
            QueryNumericCapabilities::MixedBinary64V1,
            QueryGroupSemantics::SourceAggregateV1,
            QueryLiteralSemantics::SourceLiteralV1,
        )
        .unwrap()
    }
    #[derive(Default)]
    struct HashBytes(Vec<u8>);
    impl Hasher for HashBytes {
        fn finish(&self) -> u64 {
            0
        }
        fn write(&mut self, bytes: &[u8]) {
            self.0.extend_from_slice(bytes);
        }
    }
    fn hash_bytes(value: &impl Hash) -> Vec<u8> {
        let mut state = HashBytes::default();
        value.hash(&mut state);
        state.0
    }
    fn assert_loss(error: QueryEvaluationError) {
        let QueryEvaluationError::Unexpected(error) = error else {
            panic!("expected typed loss");
        };
        assert!(matches!(
            error.downcast_ref::<RawNumericFault>(),
            Some(RawNumericFault::LossyNumericBackend)
        ));
    }
    fn assert_original_error(error: QueryEvaluationError, original: &ConversionError) {
        let QueryEvaluationError::Dataset(error) = error else {
            panic!("expected backend error");
        };
        let actual = error.downcast_ref::<ConversionError>().unwrap();
        assert!(Arc::ptr_eq(&actual.0, &original.0));
    }

    #[test]
    fn source_transport_retains_occurrence_identity_and_original_physical_hash_bytes() {
        let control = DatasetControl::new();
        let calls = control.externalizations.clone();
        let dataset = NumericDataset::new(control);
        let session = session();
        let plain_raw = session
            .construct_source_direct_string("a", None, None, true)
            .unwrap();
        let typed_raw = session
            .construct_source_direct_string(
                "a",
                Some(NamedNode::new("http://www.w3.org/2001/XMLSchema#string").unwrap()),
                None,
                true,
            )
            .unwrap();
        let plain = dataset.internalize_raw(plain_raw.clone()).unwrap();
        let typed = dataset.internalize_raw(typed_raw.clone()).unwrap();
        assert_eq!(calls.get(), 2); // real text roundtrip, once per occurrence
        assert_eq!(plain, typed); // unchanged physical RDF identity
        assert_eq!(hash_bytes(&plain), hash_bytes(plain.rdf()));
        assert_eq!(hash_bytes(&plain), hash_bytes(&typed));
        assert_ne!(
            plain.source_identity_key().unwrap(),
            typed.source_identity_key().unwrap()
        );
        assert_eq!(calls.get(), 2); // identity does not re-externalize
        assert!(plain.same_transport_instance(&plain.clone()));
        assert!(!plain.same_transport_instance(&typed));
        assert_eq!(plain.literal_source(), Some(plain_raw.literal_source()));
        let out = dataset.externalize_raw(plain).unwrap();
        assert_eq!(calls.get(), 3);
        assert_eq!(out.literal_source(), plain_raw.literal_source());
        assert_eq!(out.rdf(), plain_raw.rdf());
        let language = session
            .construct_source_direct_string("a", None, Some("EN\n".into()), true)
            .unwrap();
        let roundtrip = dataset
            .externalize_raw(dataset.internalize_raw(language.clone()).unwrap())
            .unwrap();
        assert_eq!(roundtrip.literal_source(), language.literal_source());
        assert_eq!(roundtrip.rdf(), language.rdf());
    }

    #[test]
    fn source_unknown_transport_is_lazy_but_actual_identity_consumption_refuses() {
        let control = DatasetControl::new();
        let calls = control.externalizations.clone();
        let dataset = NumericDataset::new(control);
        let raw = RawExpressionValue::from_import(Literal::new_simple_literal("a").into());
        let internal = dataset.internalize_raw(raw.clone()).unwrap();
        assert_eq!(calls.get(), 0); // no source promise to validate eagerly
        assert!(internal.same_transport_instance(&internal.clone()));
        assert!(matches!(
            internal.source_identity_key(),
            Err(RawNumericFault::UnknownSourceLiteralIdentity)
        ));
        assert_eq!(calls.get(), 0);
        let out = dataset.externalize_raw(internal).unwrap();
        assert_eq!(calls.get(), 1);
        assert_eq!(out.literal_source(), raw.literal_source());
        let scan = CachedInternalTerm::unknown(raw.rdf().unwrap().clone());
        assert_eq!(scan.literal_source(), None);
        assert!(matches!(
            scan.source_identity_key(),
            Err(RawNumericFault::UnknownSourceLiteralIdentity)
        ));
        assert!(!scan.same_transport_instance(&scan.clone()));
        let out = dataset.externalize_raw(scan).unwrap();
        assert_eq!(out.literal_source(), raw.literal_source());
        let iri = dataset
            .internalize_raw(RawExpressionValue::from_import(
                NamedNode::new("urn:source").unwrap().into(),
            ))
            .unwrap();
        assert!(matches!(
            iri.source_identity_key(),
            Ok(SourceTermKey::NamedNode(_))
        ));
    }

    #[test]
    fn source_text_rejects_loss_at_both_real_backend_boundaries() {
        let control = DatasetControl::new();
        let loss_at = control.loss_at.clone();
        let calls = control.externalizations.clone();
        let dataset = NumericDataset::new(control);
        let raw = session()
            .construct_source_direct_string("a", None, None, true)
            .unwrap();
        loss_at.set(1);
        assert_loss(dataset.internalize_raw(raw.clone()).unwrap_err());
        assert_eq!(calls.get(), 1);
        loss_at.set(0);
        let internal = dataset.internalize_raw(raw).unwrap();
        assert_eq!(calls.get(), 2);
        loss_at.set(3);
        assert_loss(dataset.externalize_raw(internal).unwrap_err());
        assert_eq!(calls.get(), 3);
    }

    #[test]
    fn source_transport_keeps_original_backend_failure_objects() {
        let control = DatasetControl::new();
        let fail_at = control.fail_at.clone();
        let calls = control.externalizations.clone();
        let original = control.error.clone();
        let dataset = NumericDataset::new(control);
        let raw = session()
            .construct_source_direct_string("a", None, None, true)
            .unwrap();
        fail_at.set(1);
        assert_original_error(dataset.internalize_raw(raw.clone()).unwrap_err(), &original);
        assert_eq!(calls.get(), 1);
        fail_at.set(0);
        let internal = dataset.internalize_raw(raw).unwrap();
        assert_eq!(calls.get(), 2);
        fail_at.set(3);
        assert_original_error(dataset.externalize_raw(internal).unwrap_err(), &original);
        assert_eq!(calls.get(), 3);
    }

    fn nonliteral_pairs() -> [(Term, Term); 2] {
        [
            (
                NamedNode::new("urn:source:a").unwrap().into(),
                NamedNode::new("urn:source:b").unwrap().into(),
            ),
            (
                oxrdf::BlankNode::new("source-a").unwrap().into(),
                oxrdf::BlankNode::new("source-b").unwrap().into(),
            ),
        ]
    }

    #[test]
    fn source_nonliteral_identity_is_checked_at_both_backend_boundaries() {
        // Independent active supplement NLB-N1/N2/B1/B2, frozen before repair.
        // This is a backend-integrity contract, not original Python behavior.
        for (input, replacement) in nonliteral_pairs() {
            for failed_call in [1, 2] {
                let mut control = DatasetControl::new();
                control.replacement = replacement.clone();
                control.loss_at.set(failed_call);
                let calls = control.externalizations.clone();
                let inputs = control.internalizations.clone();
                let dataset = NumericDataset::new_with_literal_semantics(
                    control,
                    QueryLiteralSemantics::SourceLiteralV1,
                );
                let raw = RawExpressionValue::from_import(input.clone());
                let entered = dataset.internalize_raw(raw.clone());
                assert_eq!(inputs.get(), 1);
                assert_eq!(calls.get(), 1);
                if failed_call == 1 {
                    assert_loss(entered.unwrap_err());
                } else {
                    let internal = entered.unwrap();
                    assert_eq!(
                        internal.source_identity_key().unwrap(),
                        source_identity_key(&raw).unwrap()
                    );
                    assert_eq!(calls.get(), 1);
                    assert_loss(dataset.externalize_raw(internal).unwrap_err());
                    assert_eq!(calls.get(), 2);
                }
            }
        }
    }

    #[test]
    fn source_nonliteral_success_keeps_key_without_additional_conversion() {
        // Independent active supplement NLB-N3/B3; clone must retain selection.
        for (input, _) in nonliteral_pairs() {
            let control = DatasetControl::new();
            let calls = control.externalizations.clone();
            let inputs = control.internalizations.clone();
            let dataset = NumericDataset::new_with_literal_semantics(
                control,
                QueryLiteralSemantics::SourceLiteralV1,
            )
            .clone();
            let raw = RawExpressionValue::from_import(input.clone());
            let internal = dataset.internalize_raw(raw.clone()).unwrap();
            assert_eq!(inputs.get(), 1);
            assert_eq!(calls.get(), 1);
            assert_eq!(
                internal.source_identity_key().unwrap(),
                source_identity_key(&raw).unwrap()
            );
            assert_eq!(calls.get(), 1);
            let output = dataset.externalize_raw(internal).unwrap();
            assert_eq!(calls.get(), 2);
            assert_eq!(output.rdf().unwrap(), &input);
            assert!(matches!(
                output.literal_source(),
                LiteralSourceState::NotLiteral
            ));
            assert!(matches!(output.numeric(), NumericCacheState::NotNumeric));
        }
    }

    #[test]
    fn legacy_nonliteral_constructor_keeps_prior_conversion_schedule_and_result() {
        // Independent active supplement NLB-N4/B4 protects the old lane.
        for (input, replacement) in nonliteral_pairs() {
            let mut control = DatasetControl::new();
            control.replacement = replacement.clone();
            control.loss_at.set(1);
            let calls = control.externalizations.clone();
            let inputs = control.internalizations.clone();
            let dataset = NumericDataset::new(control);
            let internal = dataset
                .internalize_raw(RawExpressionValue::from_import(input))
                .unwrap();
            assert_eq!(inputs.get(), 1);
            assert_eq!(calls.get(), 0);
            let output = dataset.externalize_raw(internal).unwrap();
            assert_eq!(calls.get(), 1);
            assert_eq!(output.rdf().unwrap(), &replacement);
            assert!(matches!(
                output.literal_source(),
                LiteralSourceState::NotLiteral
            ));
            assert!(matches!(output.numeric(), NumericCacheState::NotNumeric));
        }
    }

    #[test]
    fn generic_source_import_cannot_grant_unchecked_identity_keys() {
        let control = DatasetControl::new();
        let calls = control.externalizations.clone();
        let inputs = control.internalizations.clone();
        let dataset = NumericDataset::new_with_literal_semantics(
            control,
            QueryLiteralSemantics::SourceLiteralV1,
        );
        let internal = dataset
            .internalize_term(NamedNode::new("urn:source:a").unwrap().into())
            .unwrap();
        assert_eq!(inputs.get(), 1);
        assert_eq!(calls.get(), 0);
        assert_eq!(internal.literal_source(), None);
        assert!(matches!(
            internal.source_identity_key(),
            Err(RawNumericFault::UnknownSourceLiteralIdentity)
        ));
        assert_eq!(calls.get(), 0);
    }
}
