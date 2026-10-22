//! Independent controls for source object identity across the real codec.
//!
//! Source basis: evalutils._eval and Sample.update return the selected object;
//! term.Literal.__new__ creates a new Literal while copying an existing cached
//! value and resetting ill_typed. Unknown imports, token grants, and backend
//! integrity are explicit Rust transport policies, not Python observations.
//! No expected value is obtained by executing a candidate or a second evaluator.
use super::nullable_numeric::NumericDataset;
use super::{InternalQuad, QueryableDataset};
use crate::QueryEvaluationError;
use crate::numeric::{
    LiteralSourceState, NumericCacheState, QueryGroupSemantics, QueryLiteralSemantics,
    QueryNumericCapabilities, QueryNumericSession, RawExpressionValue, RawNumericFault, SourceFact,
    SourceHashPolicy, SourceHashRuntime,
};
use crate::tuple::{
    ErrorValue, SolutionValue, SourceException, SourceExceptionArgument, SourceExceptionClass,
    SparqlErrorKind,
};
use oxrdf::{Literal, NamedNode, Term};
use semantica_numeric::{DecimalContext, NumericLimits};
use std::cell::Cell;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::sync::Arc;

#[derive(Clone, Debug)]
struct BackendError(Arc<()>);
impl fmt::Display for BackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("source object backend marker")
    }
}
impl std::error::Error for BackendError {}

#[derive(Clone, Default)]
struct Calls {
    internalize: Rc<Cell<usize>>,
    externalize: Rc<Cell<usize>>,
    replace_at: Rc<Cell<usize>>,
    fail_at: Rc<Cell<usize>>,
}
struct Backend {
    calls: Calls,
    replacement: Term,
    error: BackendError,
}
impl Backend {
    fn new() -> Self {
        Self {
            calls: Calls::default(),
            replacement: Literal::new_simple_literal("changed by backend").into(),
            error: BackendError(Arc::new(())),
        }
    }
}
impl<'a> QueryableDataset<'a> for Backend {
    type InternalTerm = Term;
    type Error = BackendError;

    fn internal_quads_for_pattern(
        &self,
        _: Option<&Term>,
        _: Option<&Term>,
        _: Option<&Term>,
        _: Option<Option<&Term>>,
    ) -> impl Iterator<Item = Result<InternalQuad<Term>, BackendError>> + use<'a> {
        std::iter::empty()
    }
    fn internalize_term(&self, term: Term) -> Result<Term, BackendError> {
        self.calls.internalize.set(self.calls.internalize.get() + 1);
        Ok(term)
    }
    fn externalize_term(&self, term: Term) -> Result<Term, BackendError> {
        let ordinal = self.calls.externalize.get() + 1;
        self.calls.externalize.set(ordinal);
        if ordinal == self.calls.fail_at.get() {
            return Err(self.error.clone());
        }
        if ordinal == self.calls.replace_at.get() {
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

fn source_dataset(backend: Backend) -> NumericDataset<'static, Backend> {
    NumericDataset::new_with_literal_semantics(backend, QueryLiteralSemantics::SourceLiteralV1)
}

fn plain(session: &QueryNumericSession) -> RawExpressionValue {
    session
        .construct_source_direct_string("value", None, None, true)
        .unwrap()
}

fn unknown() -> RawExpressionValue {
    RawExpressionValue::from_import(Literal::new_simple_literal("unknown value").into())
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
    let mut hasher = HashBytes::default();
    value.hash(&mut hasher);
    hasher.0
}

#[test]
fn source_object_clone_and_real_codec_roundtrip_keep_reference_identity() {
    let session = session();
    let raw = plain(&session);
    let independent = plain(&session);
    assert!(raw.same_source_object(&raw.clone()));
    assert!(!raw.same_source_object(&independent));
    assert_eq!(raw, independent);
    assert_eq!(hash_bytes(&raw), hash_bytes(&independent));
    assert_eq!(hash_bytes(&raw), hash_bytes(raw.rdf().unwrap()));

    let backend = Backend::new();
    let calls = backend.calls.clone();
    let dataset = source_dataset(backend);
    let entered = dataset.internalize_raw(raw.clone()).unwrap();
    assert_eq!((calls.internalize.get(), calls.externalize.get()), (1, 1));
    assert!(entered.same_transport_instance(&entered.clone()));
    let out = dataset.externalize_raw(entered.clone()).unwrap();
    assert_eq!((calls.internalize.get(), calls.externalize.get()), (1, 2));
    assert!(raw.same_source_object(&out));
    let returned = dataset.internalize_raw(out).unwrap();
    assert_eq!((calls.internalize.get(), calls.externalize.get()), (2, 3));
    assert!(entered.same_transport_instance(&returned));
    assert_eq!(hash_bytes(&entered), hash_bytes(entered.rdf()));
}

#[test]
fn source_object_literal_copy_is_new_and_preserves_numeric_and_error_caches() {
    let session = session();
    let original = session
        .construct_source_direct_string(
            "1E+2",
            Some(NamedNode::new("http://www.w3.org/2001/XMLSchema#decimal").unwrap()),
            None,
            true,
        )
        .unwrap();
    let copy = session.copy_source_literal(&original).unwrap();
    assert!(!original.same_source_object(&copy));
    assert!(copy.same_source_object(&copy.clone()));
    assert_eq!(original.rdf().unwrap(), copy.rdf().unwrap());
    let (NumericCacheState::Known(before), NumericCacheState::Known(after)) =
        (original.numeric(), copy.numeric())
    else {
        panic!("source numeric caches are retained");
    };
    assert!(std::sync::Arc::ptr_eq(before, after));
    let (LiteralSourceState::Literal(before), LiteralSourceState::Literal(after)) =
        (original.literal_source(), copy.literal_source())
    else {
        panic!("source literal metadata");
    };
    assert_eq!(before.ill_typed, SourceFact::Known(Some(false)));
    assert_eq!(after.ill_typed, SourceFact::Known(None));

    let error = ErrorValue::with_source(
        SparqlErrorKind::OtherSparql,
        "divide by 0",
        SourceException {
            class: SourceExceptionClass::Sparql,
            argument: SourceExceptionArgument::Text("divide by 0".into()),
        },
    );
    let error_literal = session
        .construct_source_extremum_result(&SolutionValue::ReturnedError(error.clone()))
        .unwrap()
        .unwrap();
    let error_copy = session.copy_source_literal(&error_literal).unwrap();
    assert!(error_literal.same_source_object(&error_literal.clone()));
    assert!(!error_literal.same_source_object(&error_copy));
    for value in [&error_literal, &error_copy] {
        let NumericCacheState::ErrorObject(actual) = value.numeric() else {
            panic!("Literal(error) must retain the actual error object");
        };
        assert_eq!(actual, &error); // ErrorValue equality is Arc identity.
    }
}

#[test]
fn source_object_fact_reconstruction_does_not_claim_reference_transport() {
    let session = session();
    let raw = plain(&session);
    let LiteralSourceState::Literal(metadata) = raw.literal_source().clone() else {
        panic!("source literal metadata");
    };
    let new_declaration = raw.clone().with_source_metadata(metadata).unwrap();
    assert!(!raw.same_source_object(&new_declaration));
    let (rdf, cache, metadata) = raw.clone().into_source_parts().unwrap();
    let reconstructed = RawExpressionValue::from_source_parts(rdf, cache, metadata).unwrap();
    assert_eq!(raw, reconstructed);
    assert!(!reconstructed.same_source_object(&reconstructed.clone()));
    assert!(!raw.same_source_object(&reconstructed));
    let (rdf, cache) = raw.clone().into_parts().unwrap();
    let unproven = RawExpressionValue::from_parts(rdf, cache);
    assert!(!unproven.same_source_object(&unproven.clone()));

    let result = session
        .construct_source_plain_result("result".into())
        .unwrap();
    assert!(result.same_source_object(&result.clone()));
    assert!(
        !result.same_source_object(
            &session
                .construct_source_plain_result("result".into())
                .unwrap()
        )
    );
}

#[test]
fn source_object_unknown_imports_get_distinct_objects_but_selected_roundtrip_keeps_one() {
    let raw = unknown();
    assert!(!raw.same_source_object(&raw.clone()));
    let backend = Backend::new();
    let calls = backend.calls.clone();
    let dataset = source_dataset(backend);
    let left = dataset.internalize_raw(raw.clone()).unwrap();
    let right = dataset.internalize_raw(raw).unwrap();
    assert_eq!((calls.internalize.get(), calls.externalize.get()), (2, 0));
    assert!(!left.same_transport_instance(&right));
    assert!(left.same_transport_instance(&left.clone()));
    let selected = dataset.externalize_raw(left.clone()).unwrap();
    assert_eq!(calls.externalize.get(), 1);
    let returned = dataset.internalize_raw(selected).unwrap();
    assert_eq!((calls.internalize.get(), calls.externalize.get()), (3, 1));
    assert!(left.same_transport_instance(&returned));
    assert!(!right.same_transport_instance(&returned));
}

#[test]
fn source_object_identity_token_never_grants_unknown_hash_or_key_facts() {
    let backend = Backend::new();
    let calls = backend.calls.clone();
    let dataset = source_dataset(backend);
    let internal = dataset.internalize_raw(unknown()).unwrap();
    assert!(internal.same_transport_instance(&internal.clone()));
    assert!(internal.source_context_equal(&internal.clone()).unwrap());
    assert!(matches!(
        internal.source_identity_key(),
        Err(RawNumericFault::UnknownSourceLiteralIdentity)
    ));
    let hash = SourceHashRuntime::new(SourceHashPolicy::Seeded(0)).unwrap();
    assert!(matches!(
        hash.term_hash(internal.source_raw().unwrap()),
        Err(RawNumericFault::UnknownSourceLiteralIdentity)
    ));
    assert_eq!((calls.internalize.get(), calls.externalize.get()), (1, 0));
}

#[test]
fn source_object_unknown_changed_backend_is_rejected_after_actual_externalization() {
    // New active integrity policy: identity cannot survive changed RDF bytes.
    // Unknown metadata still causes no added internalization readback.
    let backend = Backend::new();
    let calls = backend.calls.clone();
    calls.replace_at.set(1);
    let dataset = source_dataset(backend);
    let internal = dataset.internalize_raw(unknown()).unwrap();
    assert_eq!((calls.internalize.get(), calls.externalize.get()), (1, 0));
    let error = dataset.externalize_raw(internal).unwrap_err();
    let QueryEvaluationError::Unexpected(error) = error else {
        panic!("expected the typed backend integrity refusal");
    };
    assert!(matches!(
        error.downcast_ref::<RawNumericFault>(),
        Some(RawNumericFault::LossyNumericBackend)
    ));
    assert_eq!((calls.internalize.get(), calls.externalize.get()), (1, 1));
}

#[test]
fn source_object_unknown_legacy_axis_keeps_reconstruction_without_new_refusal() {
    let backend = Backend::new();
    let calls = backend.calls.clone();
    let replacement = backend.replacement.clone();
    calls.replace_at.set(1);
    let dataset = NumericDataset::new(backend);
    let internal = dataset.internalize_raw(unknown()).unwrap();
    assert_eq!((calls.internalize.get(), calls.externalize.get()), (1, 0));
    let previous = internal.source_raw().unwrap().clone();
    let changed = dataset.externalize_raw(internal).unwrap();
    assert_eq!(changed.rdf().unwrap(), &replacement);
    assert!(!changed.same_source_object(&previous));
    assert!(!changed.same_source_object(&changed.clone()));
    assert!(matches!(
        changed.literal_source(),
        LiteralSourceState::Literal(metadata)
            if metadata.language_original == SourceFact::Unknown
    ));
    assert_eq!((calls.internalize.get(), calls.externalize.get()), (1, 1));
}

#[test]
fn source_object_snapshot_cannot_hide_the_original_backend_error() {
    let backend = Backend::new();
    let calls = backend.calls.clone();
    let marker = backend.error.clone();
    calls.fail_at.set(1);
    let dataset = source_dataset(backend);
    let internal = dataset.internalize_raw(unknown()).unwrap();
    assert!(internal.same_transport_instance(&internal.clone()));
    let error = dataset.externalize_raw(internal).unwrap_err();
    let QueryEvaluationError::Dataset(error) = error else {
        panic!("expected original Dataset failure");
    };
    let actual = error.downcast_ref::<BackendError>().unwrap();
    assert!(Arc::ptr_eq(&actual.0, &marker.0));
    assert_eq!((calls.internalize.get(), calls.externalize.get()), (1, 1));
}
