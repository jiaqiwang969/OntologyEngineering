//! Private per-parse literal occurrences. Handles are not RDF identities.
use crate::{NumericTokenKind, NumericTokenSign, ParsedSourceLiteral};
use std::error::Error;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::sync::Arc;

#[doc(hidden)]
#[derive(Clone, Default)]
pub struct SourceLiteralScope(Arc<()>);
impl SourceLiteralScope {
    pub fn new() -> Self {
        Self::default()
    }
    pub(crate) fn occurrence(&self, index: usize) -> SourceLiteralOccurrenceId {
        SourceLiteralOccurrenceId {
            scope: self.clone(),
            index,
        }
    }
}
impl fmt::Debug for SourceLiteralScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SourceLiteralScope(<opaque>)")
    }
}
impl PartialEq for SourceLiteralScope {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for SourceLiteralScope {}

#[doc(hidden)]
#[derive(Clone, Eq, PartialEq)]
pub struct SourceLiteralOccurrenceId {
    scope: SourceLiteralScope,
    index: usize,
}
impl SourceLiteralOccurrenceId {
    pub fn belongs_to(&self, scope: &SourceLiteralScope) -> bool {
        self.scope == *scope
    }
    pub fn index(&self) -> usize {
        self.index
    }
}
impl Hash for SourceLiteralOccurrenceId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.index.hash(state);
    }
}
impl fmt::Debug for SourceLiteralOccurrenceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SourceLiteralOccurrenceId")
            .field("scope", &self.scope)
            .field("index", &self.index)
            .finish()
    }
}

#[doc(hidden)]
pub type SourceLiteralBridgeError = Box<dyn Error + Send + Sync>;
#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SourceLiteralInput {
    Parsed(ParsedSourceLiteral),
    NumericToken {
        kind: NumericTokenKind,
        sign: NumericTokenSign,
        magnitude: String,
    },
}
#[doc(hidden)]
pub type SourceLiteralConstruct = Rc<
    dyn Fn(
        SourceLiteralOccurrenceId,
        SourceLiteralInput,
    ) -> Result<ParsedSourceLiteral, SourceLiteralBridgeError>,
>;
#[doc(hidden)]
pub type SourceLiteralTruth =
    Rc<dyn Fn(&SourceLiteralOccurrenceId) -> Result<bool, SourceLiteralBridgeError>>;

/// Source _traverseAgg/_addVars construction facts, not a query plan or oracle.
#[doc(hidden)]
#[derive(Clone, Debug)]
pub enum SourceVariableSetStep {
    New(Vec<crate::term::Variable>),
    Union(Vec<usize>),
    Copy(usize),
}
#[doc(hidden)]
pub type SourceVariableSets = Rc<
    dyn Fn(
        &[SourceVariableSetStep],
    ) -> Result<Vec<Vec<crate::term::Variable>>, SourceLiteralBridgeError>,
>;
#[doc(hidden)]
pub type SourceTriplesOrder = Rc<
    dyn Fn(
        &[crate::term::TriplePattern],
    ) -> Result<Vec<crate::term::TriplePattern>, SourceLiteralBridgeError>,
>;
#[doc(hidden)]
pub type SourcePathAdmission =
    Rc<dyn Fn(&crate::algebra::PropertyPathExpression) -> Result<(), SourceLiteralBridgeError>>;
#[doc(hidden)]
pub type SourceNativeTriplesOrder = Rc<
    dyn Fn(
        &[crate::SourceTriplePattern],
    ) -> Result<Vec<crate::SourceTriplePattern>, SourceLiteralBridgeError>,
>;
#[doc(hidden)]
pub type SourceVisibleProjection = Rc<
    dyn Fn(
        &[crate::term::Variable],
        &[Vec<crate::term::Variable>],
    ) -> Result<Vec<crate::term::Variable>, SourceLiteralBridgeError>,
>;
#[doc(hidden)]
#[derive(Clone)]
pub struct SourceLiteralHooks {
    pub scope: SourceLiteralScope,
    pub construct: SourceLiteralConstruct,
    pub python_truth: SourceLiteralTruth,
    pub variable_sets: Option<SourceVariableSets>,
    pub triples_order: Option<SourceTriplesOrder>,
    pub native_triples_order: Option<SourceNativeTriplesOrder>,
    pub graph_path_admission: Option<SourcePathAdmission>,
    pub visible_projection: Option<SourceVisibleProjection>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SourceDatatype, SourceLiteralOrigin};
    use oxrdf::Literal;

    #[derive(Default)]
    struct RecordingHasher(Vec<u8>);
    impl Hasher for RecordingHasher {
        fn finish(&self) -> u64 {
            0
        }
        fn write(&mut self, bytes: &[u8]) {
            self.0.extend_from_slice(bytes);
        }
    }

    #[test]
    fn source_literal_occurrence_keeps_query_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<crate::Query>();
        assert_send_sync::<crate::algebra::Expression>();
        assert_send_sync::<crate::algebra::GraphPattern>();
    }

    #[test]
    fn source_literal_none_preserves_previous_hash_and_debug() {
        #[derive(Debug, Hash)]
        struct ParsedSourceLiteral<'a> {
            rdf: &'a Literal,
            datatype: &'a SourceDatatype,
            language_original: &'a Option<String>,
            origin: &'a SourceLiteralOrigin,
        }
        let literal = crate::ParsedSourceLiteral {
            rdf: Literal::new_simple_literal("same"),
            datatype: SourceDatatype::Absent,
            language_original: None,
            origin: SourceLiteralOrigin::QueryPlain,
            occurrence: None,
        };
        let previous = ParsedSourceLiteral {
            rdf: &literal.rdf,
            datatype: &literal.datatype,
            language_original: &literal.language_original,
            origin: &literal.origin,
        };
        let mut old = RecordingHasher::default();
        let mut current = RecordingHasher::default();
        previous.hash(&mut old);
        literal.hash(&mut current);
        assert_eq!(old.0, current.0);
        assert_eq!(format!("{previous:?}"), format!("{literal:?}"));
    }

    #[test]
    fn source_literal_occurrence_scope_is_opaque_and_independent() {
        let scope = SourceLiteralScope::new();
        let independent = SourceLiteralScope::new();
        let first = scope.occurrence(0);
        let repeated = scope.clone().occurrence(0);
        let other = independent.occurrence(0);
        assert_eq!(first, repeated);
        assert_ne!(first, other);
        assert!(first.belongs_to(&scope));
        assert!(!first.belongs_to(&independent));
        let mut a = RecordingHasher::default();
        let mut b = RecordingHasher::default();
        first.hash(&mut a);
        other.hash(&mut b);
        assert_eq!(a.0, b.0);
        assert_eq!(
            format!("{first:?}"),
            "SourceLiteralOccurrenceId { scope: SourceLiteralScope(<opaque>), index: 0 }"
        );
    }
}
