//! Solution-domain cells shared by the existing evaluator's tuple operations.
//! Presence is separate from RDF values and from application-shadow provenance.
use rustc_hash::FxHashSet;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
#[cfg(feature = "rdflib760-nullable-internal")]
mod source_context;
#[cfg(feature = "rdflib760-nullable-internal")]
mod source_context_owner;
#[cfg(feature = "rdflib760-nullable-internal")]
use source_context::SourceTupleMetadata;
#[cfg(all(test, feature = "rdflib760-nullable-internal"))]
mod source_context_owner_receiving;
#[cfg(all(test, feature = "rdflib760-nullable-internal"))]
mod source_context_receiving;

/// An internal SPARQL error object. Clones preserve instance identity; separate
/// errors with equal diagnostics do not become the same DISTINCT value. No RDF
/// term or externally supplied identifier can impersonate this object.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SparqlErrorKind {
    NotBound,
    Type,
    OtherSparql,
}

/// Fixed source exception constructors, distinct from diagnostic text. The
/// original SPARQLError constructor always passes one argument to Exception,
/// including its default None argument (whose str is "None").
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SourceExceptionClass {
    Sparql,
    SparqlType,
    NotBound,
    AlreadyBound,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SourceExceptionArgument {
    None,
    Text(String),
    #[cfg(feature = "rdflib760-nullable-internal")]
    DecimalSignalClasses(Vec<semantica_numeric::DecimalSignal>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SourceException {
    pub(crate) class: SourceExceptionClass,
    pub(crate) argument: SourceExceptionArgument,
}

#[derive(Debug)]
struct ErrorPayload {
    kind: SparqlErrorKind,
    diagnostic: Box<str>,
    source: Option<SourceException>,
}

#[derive(Clone, Debug)]
pub(crate) struct ErrorValue(Arc<ErrorPayload>);

impl ErrorValue {
    pub(crate) fn new(diagnostic: &str) -> Self {
        Self::with_kind(SparqlErrorKind::OtherSparql, diagnostic)
    }

    pub(crate) fn with_kind(kind: SparqlErrorKind, diagnostic: &str) -> Self {
        Self(Arc::new(ErrorPayload {
            kind,
            diagnostic: diagnostic.into(),
            source: None,
        }))
    }

    pub(crate) fn with_source(
        kind: SparqlErrorKind,
        diagnostic: &str,
        source: SourceException,
    ) -> Self {
        Self(Arc::new(ErrorPayload {
            kind,
            diagnostic: diagnostic.into(),
            source: Some(source),
        }))
    }

    pub(crate) fn source_exception(&self) -> Option<&SourceException> {
        self.0.source.as_ref()
    }

    pub(crate) fn kind(&self) -> SparqlErrorKind {
        self.0.kind
    }

    pub(crate) fn diagnostic(&self) -> &str {
        &self.0.diagnostic
    }
    #[cfg(feature = "rdflib760-nullable-internal")]
    pub(crate) fn source_native_identity_hash(&self) -> i64 {
        crate::source_hash::hash_source_pointer_word(Arc::as_ptr(&self.0) as usize as u64)
    }
}
impl PartialEq for ErrorValue {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for ErrorValue {}
impl Hash for ErrorValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.0).hash(state);
    }
}

/// Absence is the outer None in InternalTuple. ExplicitNone and ReturnedError
/// are present local keys, without being RDF terms.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub(crate) enum SolutionValue<T> {
    Rdf(T),
    ExplicitNone,
    ReturnedError(ErrorValue),
}

impl<T> SolutionValue<T> {
    /// Map only RDF payloads; explicit None and returned error objects are
    /// real cells, and clones retain the same error instance.
    pub(crate) fn try_map_rdf<U, E>(
        &self,
        mut map: impl FnMut(&T) -> Result<U, E>,
    ) -> Result<SolutionValue<U>, E> {
        Ok(match self {
            Self::Rdf(value) => SolutionValue::Rdf(map(value)?),
            Self::ExplicitNone => SolutionValue::ExplicitNone,
            Self::ReturnedError(error) => SolutionValue::ReturnedError(error.clone()),
        })
    }

    pub(crate) fn rdf(&self) -> Option<&T> {
        match self {
            Self::Rdf(value) => Some(value),
            Self::ExplicitNone | Self::ReturnedError(_) => None,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct InternalTuple<T> {
    inner: Vec<Option<SolutionValue<T>>>,
    // Computed-origin scope, not RDF identity or a solution-domain column.
    initial_shadows: FxHashSet<usize>,
    // Storage facts, not preparation authority. Only the sealed source-context
    // compiler may use these for original binding-context execution.
    #[cfg(feature = "rdflib760-nullable-internal")]
    source: SourceTupleMetadata,
    #[cfg(feature = "rdflib760-nullable-internal")]
    source_context_owner: Option<std::rc::Rc<source_context_owner::SourceContextOwner<T>>>,
}

impl<T> InternalTuple<T> {
    /// Explicit fallible identity view. This does not alter physical Eq/Hash,
    /// include computed shadows, compact absent columns, or externalize RDF.
    pub(crate) fn try_identity_key<K, E>(
        &self,
        mut key: impl FnMut(&T) -> Result<K, E>,
    ) -> Result<Vec<Option<SolutionValue<K>>>, E> {
        self.cells()
            .map(|cell| cell.map(|cell| cell.try_map_rdf(&mut key)).transpose())
            .collect()
    }

    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: Vec::with_capacity(capacity),
            initial_shadows: FxHashSet::default(),
            #[cfg(feature = "rdflib760-nullable-internal")]
            source: SourceTupleMetadata::new(),
            #[cfg(feature = "rdflib760-nullable-internal")]
            source_context_owner: None,
        }
    }

    pub(crate) fn capacity(&self) -> usize {
        self.inner.capacity()
    }

    /// RDF-only lookup for existing term-consuming boundaries. Operations on
    /// solution domains must use cell() or contains_cell(), not this method.
    pub(crate) fn get(&self, index: usize) -> Option<&T> {
        self.cell(index).and_then(SolutionValue::rdf)
    }

    pub(crate) fn contains(&self, index: usize) -> bool {
        self.get(index).is_some()
    }

    pub(crate) fn cell(&self, index: usize) -> Option<&SolutionValue<T>> {
        self.inner.get(index).and_then(Option::as_ref)
    }

    pub(crate) fn contains_cell(&self, index: usize) -> bool {
        self.cell(index).is_some()
    }

    pub(crate) fn is_initial_shadow(&self, index: usize) -> bool {
        self.initial_shadows.contains(&index)
    }

    pub(crate) fn set_cell(&mut self, index: usize, value: SolutionValue<T>) {
        #[cfg(feature = "rdflib760-nullable-internal")]
        {
            let present = self.contains_cell(index);
            self.source.record_set(index, present);
        }
        if self.inner.len() <= index {
            self.inner.resize_with(index + 1, || None);
        }
        self.inner[index] = Some(value);
        self.initial_shadows.remove(&index);
    }

    pub(crate) fn set(&mut self, index: usize, value: T) {
        self.set_cell(index, SolutionValue::Rdf(value));
    }

    pub(crate) fn set_cell_with_shadow(
        &mut self,
        index: usize,
        value: SolutionValue<T>,
        shadow: bool,
    ) {
        self.set_cell(index, value);
        if shadow {
            self.initial_shadows.insert(index);
        }
    }

    pub(crate) fn set_with_shadow(&mut self, index: usize, value: T, shadow: bool) {
        self.set_cell_with_shadow(index, SolutionValue::Rdf(value), shadow);
    }

    pub(crate) fn remove_cell(&mut self, index: usize) {
        #[cfg(feature = "rdflib760-nullable-internal")]
        if self.contains_cell(index) {
            self.source.record_remove(index);
        }
        if let Some(value) = self.inner.get_mut(index) {
            *value = None;
            self.initial_shadows.remove(&index);
            // Capacity and trailing absent slots are not dictionary identity.
            while self.inner.last().is_some_and(Option::is_none) {
                self.inner.pop();
            }
        }
    }

    pub(crate) fn cells(&self) -> impl Iterator<Item = Option<&SolutionValue<T>>> {
        self.inner.iter().map(Option::as_ref)
    }

    pub(crate) fn into_cells(self) -> impl Iterator<Item = Option<SolutionValue<T>>> {
        self.inner.into_iter()
    }

    pub(crate) fn domain_disjoint(&self, other: &Self) -> bool {
        !self
            .inner
            .iter()
            .enumerate()
            .any(|(index, value)| value.is_some() && other.contains_cell(index))
    }
}

impl<T: Clone> InternalTuple<T> {
    /// Explicit source-aware compatibility. Callers choose the equality
    /// contract; a missing key does not force the other cell's identity.
    pub(crate) fn try_compatible_cells_by<E>(
        &self,
        other: &Self,
        mut equals: impl FnMut(&T, &T) -> Result<bool, E>,
    ) -> Result<bool, E> {
        for (index, value) in self.cells().enumerate() {
            if let (Some(left), Some(right)) = (value, other.cell(index)) {
                if !try_cells_equal_by(left, right, &mut equals)? {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    pub(crate) fn try_combine_with_by<E>(
        &self,
        other: &Self,
        equals: impl FnMut(&T, &T) -> Result<bool, E>,
    ) -> Result<Option<Self>, E> {
        if !self.try_compatible_cells_by(other, equals)? {
            return Ok(None);
        }
        let (base, extra) = if self.inner.len() < other.inner.len() {
            (other, self)
        } else {
            (self, other)
        };
        let mut out = base.clone();
        for (index, value) in extra.cells().enumerate() {
            if !out.contains_cell(index) {
                if let Some(value) = value {
                    out.set_cell(index, value.clone());
                }
            }
        }
        out.initial_shadows
            .extend(self.initial_shadows.iter().copied());
        out.initial_shadows
            .extend(other.initial_shadows.iter().copied());
        Ok(Some(out))
    }

    /// Source-aware counterpart of the existing computed-shadow restoration;
    /// this does not introduce or claim complete source context thaw.
    pub(crate) fn try_restore_initial_with_shadows_by<E>(
        &self,
        context: &Self,
        mut equals: impl FnMut(&T, &T) -> Result<bool, E>,
    ) -> Result<Option<Self>, E> {
        let mut result = self.clone();
        for (index, initial) in context.cells().enumerate() {
            if let Some(initial) = initial {
                match self.cell(index) {
                    Some(_) if self.is_initial_shadow(index) => (),
                    Some(existing) => {
                        if !try_cells_equal_by(existing, initial, &mut equals)? {
                            return Ok(None);
                        }
                    }
                    None => result.set_cell(index, initial.clone()),
                }
            }
        }
        Ok(Some(result))
    }

    /// Raw FrozenBindings variable lookup: a present None masks application
    /// context; only a missing key may fall back. Context thaw is separate.
    pub(crate) fn lookup_cell(&self, index: usize, initial: &Self) -> Option<SolutionValue<T>> {
        self.cell(index).or_else(|| initial.cell(index)).cloned()
    }

    pub(crate) fn project_cells(&self, columns: &[usize]) -> Self {
        let mut out = Self::with_capacity(self.capacity());
        for &index in columns {
            if let Some(value) = self.cell(index) {
                out.set_cell_with_shadow(index, value.clone(), self.is_initial_shadow(index));
            }
        }
        out
    }

    /// Right local keys overwrite. The caller owns join compatibility and
    /// context identity; this is not an implicit thaw or a lazy-join algorithm.
    pub(crate) fn merge_cells(&self, right: &Self) -> Self {
        let mut out = self.clone();
        for (index, value) in right.cells().enumerate() {
            if let Some(value) = value {
                out.set_cell_with_shadow(index, value.clone(), right.is_initial_shadow(index));
            }
        }
        out
    }
}

fn try_cells_equal_by<T, E>(
    left: &SolutionValue<T>,
    right: &SolutionValue<T>,
    equals: &mut impl FnMut(&T, &T) -> Result<bool, E>,
) -> Result<bool, E> {
    Ok(match (left, right) {
        (SolutionValue::Rdf(left), SolutionValue::Rdf(right)) => equals(left, right)?,
        (SolutionValue::ExplicitNone, SolutionValue::ExplicitNone) => true,
        (SolutionValue::ReturnedError(left), SolutionValue::ReturnedError(right)) => left == right,
        _ => false,
    })
}

impl<T: Clone + Eq> InternalTuple<T> {
    pub(crate) fn compatible_cells(&self, other: &Self) -> bool {
        self.cells()
            .enumerate()
            .all(|(index, value)| match (value, other.cell(index)) {
                (Some(left), Some(right)) => left == right,
                _ => true,
            })
    }

    /// The existing ordinary join now preserves every actual cell. RDFLib's
    /// asymmetric fallback and lazy-context algorithms remain distinct work.
    pub(crate) fn combine_with(&self, other: &Self) -> Option<Self> {
        if !self.compatible_cells(other) {
            return None;
        }
        // Preserve the original evaluator's representation choice for equal
        // terms: retain the longer tuple's cell, or the left cell on a tie.
        // Eq does not require an encoded dataset term's representation to be
        // identical. FrozenBindings' overwrite merge is a different operation.
        let (base, extra) = if self.inner.len() < other.inner.len() {
            (other, self)
        } else {
            (self, other)
        };
        let mut out = base.clone();
        for (index, value) in extra.cells().enumerate() {
            if !out.contains_cell(index) {
                if let Some(value) = value {
                    out.set_cell(index, value.clone());
                }
            }
        }
        out.initial_shadows
            .extend(self.initial_shadows.iter().copied());
        out.initial_shadows
            .extend(other.initial_shadows.iter().copied());
        Some(out)
    }

    /// Original /3 computed-shadow restoration. Do not reuse this as /5 thaw.
    pub(crate) fn restore_initial_with_shadows(&self, context: &Self) -> Option<Self> {
        let mut result = self.clone();
        for (index, initial) in context.cells().enumerate() {
            if let Some(initial) = initial {
                match self.cell(index) {
                    Some(_) if self.is_initial_shadow(index) => (),
                    Some(existing) if existing != initial => return None,
                    Some(_) => (),
                    None => result.set_cell(index, initial.clone()),
                }
            }
        }
        Some(result)
    }
}

impl<T: Eq> PartialEq for InternalTuple<T> {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}
impl<T: Eq> Eq for InternalTuple<T> {}
impl<T: Hash> Hash for InternalTuple<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.inner.hash(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::hash_map::DefaultHasher;
    type Row = InternalTuple<&'static str>;

    fn row(code: char) -> Row {
        let mut row = Row::with_capacity(3);
        match code {
            'A' => (),
            'N' => row.set_cell(0, SolutionValue::ExplicitNone),
            'T' => row.set(0, "urn:t"),
            'U' => row.set(0, "urn:u"),
            _ => panic!("invalid source-derived fixture"),
        }
        row
    }
    fn digest(row: &Row) -> u64 {
        let mut h = DefaultHasher::new();
        row.hash(&mut h);
        h.finish()
    }

    #[test]
    fn nullable_cells_keep_frozen_compatibility_domain_and_lookup_tables() {
        // Literal independent table frozen from sparql.py before implementation.
        let compatible = [
            true, true, true, true, true, true, false, false, true, false, true, false, true,
            false, false, true,
        ];
        for (i, left) in ['A', 'N', 'T', 'U'].into_iter().enumerate() {
            for (j, right) in ['A', 'N', 'T', 'U'].into_iter().enumerate() {
                assert_eq!(
                    row(left).combine_with(&row(right)).is_some(),
                    compatible[4 * i + j]
                );
            }
        }
        let intersections = [false, false, false, false, true, true, false, true, true];
        for (i, left) in ['A', 'N', 'T'].into_iter().enumerate() {
            for (j, right) in ['A', 'N', 'T'].into_iter().enumerate() {
                assert_eq!(
                    !row(left).domain_disjoint(&row(right)),
                    intersections[3 * i + j]
                );
            }
        }
        assert_eq!(
            row('A').lookup_cell(0, &row('T')),
            Some(SolutionValue::Rdf("urn:t"))
        );
        assert_eq!(
            row('N').lookup_cell(0, &row('T')),
            Some(SolutionValue::ExplicitNone)
        );
        assert_eq!(
            row('U').lookup_cell(0, &row('T')),
            Some(SolutionValue::Rdf("urn:u"))
        );
        assert_eq!(row('N').get(0), None);
        assert!(row('N').contains_cell(0));
        assert!(!row('N').contains(0));
    }

    #[test]
    fn nullable_projection_hash_and_error_identity_do_not_collapse_to_absence() {
        let mut value = row('N');
        value.set(2, "urn:u");
        assert_eq!(value.project_cells(&[0, 1]), row('N'));
        assert_ne!(value.project_cells(&[0]), row('A'));
        let mut padded = row('N');
        padded.set(100, "temporary");
        padded.remove_cell(100);
        assert_eq!(padded, row('N'));
        assert_eq!(digest(&padded), digest(&row('N')));
        padded.remove_cell(0);
        assert_eq!(padded, row('A'));
        let error = ErrorValue::new("same diagnostic");
        let mut first = row('A');
        first.set_cell(0, SolutionValue::ReturnedError(error.clone()));
        let mut same = row('A');
        same.set_cell(0, SolutionValue::ReturnedError(error));
        let mut other = row('A');
        other.set_cell(
            0,
            SolutionValue::ReturnedError(ErrorValue::new("same diagnostic")),
        );
        assert_eq!(first, same);
        assert_eq!(digest(&first), digest(&same));
        assert_ne!(first, other);
        assert_ne!(first, row('N'));
        assert_ne!(first, row('A'));
    }

    #[test]
    fn ordinary_join_retains_the_original_equal_term_representation_choice() {
        #[derive(Clone, Debug)]
        struct EncodedTerm(u8, &'static str);
        impl PartialEq for EncodedTerm {
            fn eq(&self, other: &Self) -> bool {
                self.0 == other.0
            }
        }
        impl Eq for EncodedTerm {}

        let mut left = InternalTuple::with_capacity(5);
        left.set_with_shadow(0, EncodedTerm(1, "left representation"), true);
        let mut right = InternalTuple::with_capacity(1);
        right.set(0, EncodedTerm(1, "right representation"));
        let joined = left.combine_with(&right).unwrap();
        assert_eq!(joined.get(0).unwrap().1, "left representation");
        assert!(joined.is_initial_shadow(0));
        right.set(2, EncodedTerm(2, "additional value"));
        let joined = left.combine_with(&right).unwrap();
        assert_eq!(joined.get(0).unwrap().1, "right representation");
        assert!(joined.is_initial_shadow(0));
        left.set_cell(4, SolutionValue::ExplicitNone);
        let joined = left.combine_with(&right).unwrap();
        assert_eq!(joined.get(0).unwrap().1, "left representation");
        assert!(matches!(joined.cell(4), Some(SolutionValue::ExplicitNone)));
        assert_eq!(joined.get(2).unwrap().1, "additional value");
        // Explicit overwrite merge still chooses the right representation.
        assert_eq!(
            left.merge_cells(&right).get(0).unwrap().1,
            "right representation"
        );
    }

    #[test]
    fn shadow_origin_is_not_term_identity_and_context_restore_is_scoped() {
        let mut ordinary = Row::with_capacity(3);
        ordinary.set(0, "computed");
        let mut shadow = ordinary.clone();
        shadow.set_with_shadow(0, "computed", true);
        assert_eq!(ordinary, shadow);
        assert_eq!(digest(&ordinary), digest(&shadow));
        let mut context = Row::with_capacity(3);
        context.set(0, "prior");
        context.set(2, "hidden");
        assert!(ordinary.restore_initial_with_shadows(&context).is_none());
        assert!(shadow.combine_with(&context).is_none());
        let restored = shadow.restore_initial_with_shadows(&context).unwrap();
        assert_eq!(restored.get(0), Some(&"computed"));
        assert_eq!(restored.get(2), Some(&"hidden"));
        assert!(restored.is_initial_shadow(0));
        assert!(!restored.is_initial_shadow(2));
        let mut failed_after_success = restored.clone();
        assert!(failed_after_success.is_initial_shadow(0));
        failed_after_success.set(0, "computed");
        assert!(!failed_after_success.is_initial_shadow(0));
    }
}

#[cfg(test)]
mod fallible_identity_tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn fallible_tuple_key_preserves_absence_none_and_error_instance() {
        let error = ErrorValue::new("same words");
        let mut row = InternalTuple::with_capacity(8);
        row.set(0, "rdf");
        row.set_cell(2, SolutionValue::ExplicitNone);
        row.set_cell(3, SolutionValue::ReturnedError(error.clone()));
        let calls = Cell::new(0);
        let key = row
            .try_identity_key(|value| -> Result<_, &'static str> {
                calls.set(calls.get() + 1);
                Ok(value.len())
            })
            .unwrap();
        assert_eq!(
            key,
            vec![
                Some(SolutionValue::Rdf(3)),
                None,
                Some(SolutionValue::ExplicitNone),
                Some(SolutionValue::ReturnedError(error.clone()))
            ]
        );
        assert_eq!(calls.get(), 1);
        assert_ne!(
            key[3],
            Some(SolutionValue::ReturnedError(ErrorValue::new("same words")))
        );
        let padded = row.clone();
        assert_eq!(
            padded.try_identity_key(|v| Ok::<_, ()>(v.len())).unwrap(),
            key
        );
        row.set(1, "unknown");
        assert_eq!(
            row.try_identity_key(|v| if *v == "unknown" {
                Err("unknown source identity")
            } else {
                Ok(v.len())
            }),
            Err("unknown source identity")
        );
    }

    #[test]
    fn fallible_tuple_compatibility_and_merge_stop_at_first_required_identity() {
        #[derive(Clone, Debug, PartialEq, Eq)]
        struct CellValue {
            physical: &'static str,
            source: Option<u8>,
        }
        let mut left = InternalTuple::with_capacity(4);
        left.set(
            0,
            CellValue {
                physical: "same RDF",
                source: Some(1),
            },
        );
        let mut right = InternalTuple::with_capacity(4);
        right.set(
            0,
            CellValue {
                physical: "same RDF",
                source: Some(2),
            },
        );
        let equals = |a: &CellValue, b: &CellValue| -> Result<bool, &'static str> {
            Ok(a.source.ok_or("unknown")? == b.source.ok_or("unknown")?)
        };
        assert!(!left.try_compatible_cells_by(&right, equals).unwrap());
        assert!(left.try_combine_with_by(&right, equals).unwrap().is_none());
        right.set(
            0,
            CellValue {
                physical: "different representation",
                source: Some(1),
            },
        );
        right.set_with_shadow(
            2,
            CellValue {
                physical: "extra",
                source: None,
            },
            true,
        );
        let combined = left.try_combine_with_by(&right, equals).unwrap().unwrap();
        assert_eq!(
            combined.get(0).unwrap().physical,
            "different representation"
        );
        assert!(combined.is_initial_shadow(2));
        left.set(
            0,
            CellValue {
                physical: "same RDF",
                source: None,
            },
        );
        let calls = Cell::new(0);
        let error = left
            .try_combine_with_by(&right, |a, b| {
                calls.set(calls.get() + 1);
                equals(a, b)
            })
            .unwrap_err();
        assert_eq!(error, "unknown");
        assert_eq!(calls.get(), 1);
        // Missing slots transport unknown values without consuming identity.
        let empty = InternalTuple::with_capacity(0);
        assert!(
            left.try_combine_with_by(&empty, |_, _| -> Result<bool, ()> {
                panic!("no overlapping key")
            })
            .unwrap()
            .is_some()
        );
    }

    #[test]
    fn fallible_initial_restoration_preserves_shadows_without_inventing_thaw() {
        let mut row = InternalTuple::with_capacity(3);
        row.set_with_shadow(0, "computed", true);
        row.set_cell(1, SolutionValue::ExplicitNone);
        let mut context = InternalTuple::with_capacity(3);
        context.set(0, "application");
        context.set_cell(1, SolutionValue::ExplicitNone);
        context.set(2, "unknown transport");
        let out = row
            .try_restore_initial_with_shadows_by(&context, |_, _| -> Result<bool, ()> {
                panic!("shadow/None/absence need no RDF comparison")
            })
            .unwrap()
            .unwrap();
        assert_eq!(out.get(0), Some(&"computed"));
        assert_eq!(out.get(2), Some(&"unknown transport"));
        assert!(out.is_initial_shadow(0));
        row.set(0, "unknown transport");
        assert_eq!(
            row.try_restore_initial_with_shadows_by(&context, |_, _| Err::<bool, _>(
                "unknown identity"
            )),
            Err("unknown identity")
        );
    }
}
