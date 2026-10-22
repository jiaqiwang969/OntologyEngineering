//! Independent author controls for context policy and lazy identity facts.
//!
//! Registered under eval so the real private preparation gate is exercised.
//! These are active configuration/transport tests, not new original-query
//! expectations. Hash mathematics has separate frozen original receivers.
//! Native identities are owned by this Rust process; there is presently no
//! observed-Python-object witness API or historical address recovery. The
//! source hash kernel fixes the independently observed 64-bit little-endian ABI.
use super::numeric_query::{NumericPrepareError, prepare_numeric_query};
use crate::QueryEvaluator;
use crate::dataset::nullable_numeric::NumericDataset;
use crate::dataset::{InternalQuad, QueryableDataset};
use crate::numeric::{
    LiteralSourceState, NumericCacheState, QueryContextSemantics, QueryGroupSemantics,
    QueryLiteralSemantics, QueryNumericCapabilities, QueryNumericSession, RawExpressionValue,
    RawNumericFault, SourceFact, SourceHashPolicy, SourceHashRuntime, source_context_equal,
};
use crate::tuple::{
    ErrorValue, SourceException, SourceExceptionArgument, SourceExceptionClass, SparqlErrorKind,
};
use oxrdf::{Literal, NamedNode, Term};
use semantica_numeric::{DecimalContext, DecimalSignals, NumericLimits};
use spargebra::SparqlParser;
use std::cell::Cell;
use std::convert::Infallible;
use std::rc::Rc;

fn session(
    literal: QueryLiteralSemantics,
    context: QueryContextSemantics,
    runtime: Option<Rc<SourceHashRuntime>>,
) -> Result<QueryNumericSession, RawNumericFault> {
    QueryNumericSession::new_with_context_semantics(
        DecimalContext::default(),
        NumericLimits::default(),
        None,
        true,
        QueryNumericCapabilities::MixedBinary64V1,
        QueryGroupSemantics::SourceAggregateV1,
        literal,
        context,
        runtime,
    )
}

fn source_session() -> QueryNumericSession {
    session(
        QueryLiteralSemantics::SourceLiteralV1,
        QueryContextSemantics::SourceContextV1,
        Some(Rc::new(
            SourceHashRuntime::new(SourceHashPolicy::Seeded(0)).unwrap(),
        )),
    )
    .unwrap()
}

#[derive(Clone, Default)]
struct Calls {
    internalize: Rc<Cell<usize>>,
    externalize: Rc<Cell<usize>>,
    scans: Rc<Cell<usize>>,
}
struct Dataset(Calls);
impl<'a> QueryableDataset<'a> for Dataset {
    type InternalTerm = Term;
    type Error = Infallible;
    fn internal_quads_for_pattern(
        &self,
        _: Option<&Term>,
        _: Option<&Term>,
        _: Option<&Term>,
        _: Option<Option<&Term>>,
    ) -> impl Iterator<Item = Result<InternalQuad<Term>, Infallible>> + use<'a> {
        self.0.scans.set(self.0.scans.get() + 1);
        std::iter::empty()
    }
    fn internalize_term(&self, term: Term) -> Result<Term, Infallible> {
        self.0.internalize.set(self.0.internalize.get() + 1);
        Ok(term)
    }
    fn externalize_term(&self, term: Term) -> Result<Term, Infallible> {
        self.0.externalize.set(self.0.externalize.get() + 1);
        Ok(term)
    }
}

#[test]
fn source_context_session_rejects_inconsistent_version_hash_pairs() {
    let runtime = Rc::new(SourceHashRuntime::new(SourceHashPolicy::Seeded(1)).unwrap());
    for (literal, context, hash) in [
        (
            QueryLiteralSemantics::RdfIdentityV1,
            QueryContextSemantics::SourceContextV1,
            Some(Rc::clone(&runtime)),
        ),
        (
            QueryLiteralSemantics::SourceLiteralV1,
            QueryContextSemantics::SourceContextV1,
            None,
        ),
        (
            QueryLiteralSemantics::SourceLiteralV1,
            QueryContextSemantics::LegacyPrivateV1,
            Some(Rc::clone(&runtime)),
        ),
    ] {
        assert!(matches!(
            session(literal, context, hash),
            Err(RawNumericFault::InvalidSourceOrigin)
        ));
    }
    let current = session(
        QueryLiteralSemantics::SourceLiteralV1,
        QueryContextSemantics::SourceContextV1,
        Some(Rc::clone(&runtime)),
    )
    .unwrap();
    assert!(Rc::ptr_eq(current.source_hash().unwrap(), &runtime));
    let old = QueryNumericSession::new_with_literal_semantics(
        DecimalContext::default(),
        NumericLimits::default(),
        None,
        true,
        QueryNumericCapabilities::MixedBinary64V1,
        QueryGroupSemantics::SourceAggregateV1,
        QueryLiteralSemantics::SourceLiteralV1,
    )
    .unwrap();
    assert_eq!(
        old.context_semantics(),
        QueryContextSemantics::LegacyPrivateV1
    );
    assert!(old.source_hash().is_none());
}

#[test]
fn source_context_native_secret_is_process_stable_and_debug_is_policy_only() {
    let one = SourceHashRuntime::new(SourceHashPolicy::NativeRandom).unwrap();
    let expected: Vec<_> = ["a", "猫", "🙂", "a\0b"]
        .map(|s| one.variable_hash(s))
        .into();
    let handles: Vec<_> = (0..4)
        .map(|_| {
            std::thread::spawn(|| {
                let runtime = SourceHashRuntime::new(SourceHashPolicy::NativeRandom).unwrap();
                ["a", "猫", "🙂", "a\0b"].map(|s| runtime.variable_hash(s))
            })
        })
        .collect();
    for handle in handles {
        assert_eq!(handle.join().unwrap().as_slice(), expected);
    }
    assert_eq!(
        format!("{one:?}"),
        "SourceHashRuntime { policy: NativeRandom, .. }"
    );
    let session = session(
        QueryLiteralSemantics::SourceLiteralV1,
        QueryContextSemantics::SourceContextV1,
        Some(Rc::new(one)),
    )
    .unwrap();
    assert!(!format!("{session:?}").contains("secret"));
}

#[test]
fn source_context_seeded_runtime_repeats_without_claiming_old_process_identity() {
    for seed in [0, 1, u32::MAX] {
        let a = SourceHashRuntime::new(SourceHashPolicy::Seeded(seed)).unwrap();
        let b = SourceHashRuntime::new(SourceHashPolicy::Seeded(seed)).unwrap();
        for name in ["", "x", "猫🙂", "a\0b"] {
            assert_eq!(a.variable_hash(name), b.variable_hash(name));
        }
        assert_eq!(
            format!("{a:?}"),
            format!("SourceHashRuntime {{ policy: Seeded({seed}), .. }}")
        );
    }
    let error = ErrorValue::with_source(
        SparqlErrorKind::OtherSparql,
        "marker",
        SourceException {
            class: SourceExceptionClass::Sparql,
            argument: SourceExceptionArgument::Text("marker".into()),
        },
    );
    let cloned = error.clone();
    let distinct = ErrorValue::with_source(
        SparqlErrorKind::OtherSparql,
        "marker",
        error.source_exception().unwrap().clone(),
    );
    assert!(error == cloned);
    assert!(error != distinct);
    assert_eq!(
        error.source_native_identity_hash(),
        cloned.source_native_identity_hash()
    );
    assert_eq!(
        error.source_native_identity_hash(),
        error.source_native_identity_hash()
    );
    // No assertion makes a Rust address/hash equal to a prior Python address.
}

#[test]
fn source_context_hash_ignores_numeric_cache_scale_and_ill_typed_facts() {
    let session = source_session();
    let datatype = NamedNode::new("http://www.w3.org/2001/XMLSchema#decimal").unwrap();
    let exponent = session
        .construct_source_direct_string("1E+2", Some(datatype.clone()), None, true)
        .unwrap();
    let ordinary = session
        .construct_source_direct_string("100", Some(datatype), None, true)
        .unwrap();
    assert_eq!(exponent.rdf().unwrap(), ordinary.rdf().unwrap());
    let (NumericCacheState::Known(a), NumericCacheState::Known(b)) =
        (exponent.numeric(), ordinary.numeric())
    else {
        panic!("known numeric caches");
    };
    assert_ne!(a.value(), b.value());
    let LiteralSourceState::Literal(mut metadata) = exponent.literal_source().clone() else {
        panic!("literal");
    };
    metadata.ill_typed = SourceFact::Unknown;
    let partly_known = exponent.clone().with_source_metadata(metadata).unwrap();
    let runtime = session.source_hash().unwrap();
    assert_eq!(
        runtime.term_hash(&exponent).unwrap(),
        runtime.term_hash(&ordinary).unwrap()
    );
    assert_eq!(
        runtime.term_hash(&exponent).unwrap(),
        runtime.term_hash(&partly_known).unwrap()
    );
    assert!(source_context_equal(&exponent, &ordinary).unwrap());
    assert!(source_context_equal(&partly_known, &ordinary).unwrap());
    assert_eq!(session.flags(), DecimalSignals::EMPTY);
}

#[test]
fn source_context_identity_reads_only_reached_metadata_and_preserves_instance_shortcut() {
    let session = source_session();
    let unknown = RawExpressionValue::from_import(Literal::new_simple_literal("v").into());
    let iri = RawExpressionValue::from_import(NamedNode::new("urn:v").unwrap().into());
    assert!(!source_context_equal(&unknown, &iri).unwrap());
    assert!(matches!(
        source_context_equal(&unknown, &unknown),
        Err(RawNumericFault::UnknownSourceLiteralIdentity)
    ));
    let plain = session
        .construct_source_direct_string("v", None, None, false)
        .unwrap();
    let LiteralSourceState::Literal(mut partial) = plain.literal_source().clone() else {
        panic!("literal");
    };
    partial.language_original = SourceFact::Unknown;
    let partial = plain.clone().with_source_metadata(partial).unwrap();
    let typed = session
        .construct_source_direct_string(
            "v",
            Some(NamedNode::new("http://www.w3.org/2001/XMLSchema#string").unwrap()),
            None,
            false,
        )
        .unwrap();
    assert!(!source_context_equal(&partial, &typed).unwrap());
    assert!(matches!(
        source_context_equal(&partial, &plain),
        Err(RawNumericFault::UnknownSourceLiteralIdentity)
    ));
    assert!(matches!(
        session.source_hash().unwrap().term_hash(&partial),
        Err(RawNumericFault::UnknownSourceLiteralIdentity)
    ));
    let calls = Calls::default();
    let dataset = NumericDataset::new_with_literal_semantics(
        Dataset(calls.clone()),
        QueryLiteralSemantics::SourceLiteralV1,
    );
    let first = dataset.internalize_raw(unknown.clone()).unwrap();
    let distinct = dataset.internalize_raw(unknown).unwrap();
    let before = calls.externalize.get();
    assert!(first.source_context_equal(&first.clone()).unwrap());
    assert!(matches!(
        first.source_context_equal(&distinct),
        Err(RawNumericFault::UnknownSourceLiteralIdentity)
    ));
    assert_eq!(calls.externalize.get(), before);
    assert_eq!(calls.scans.get(), 0);
}

#[test]
fn source_context_prepare_runs_source_parser_before_initial_or_dataset() {
    let calls = Calls::default();
    let session = Rc::new(source_session());
    let result = prepare_numeric_query(
        "deliberately invalid query syntax",
        Dataset(calls.clone()),
        session,
        Vec::new(),
    );
    assert!(matches!(result, Err(NumericPrepareError::Syntax(_))));
    assert_eq!(calls.internalize.get(), 0);
    assert_eq!(calls.externalize.get(), 0);
    assert_eq!(calls.scans.get(), 0);
    let old = Rc::new(
        QueryNumericSession::new_with_literal_semantics(
            DecimalContext::default(),
            NumericLimits::default(),
            None,
            true,
            QueryNumericCapabilities::MixedBinary64V1,
            QueryGroupSemantics::SourceAggregateV1,
            QueryLiteralSemantics::SourceLiteralV1,
        )
        .unwrap(),
    );
    assert!(
        prepare_numeric_query(
            "SELECT ?x WHERE { VALUES ?x { 1 } }",
            Dataset(Calls::default()),
            old,
            Vec::new()
        )
        .is_ok()
    );
}

#[test]
fn source_context_annotated_plan_is_rejected_by_default_evaluator_before_dataset() {
    let constructor = Rc::new(
        |_, _, _: &str| -> Result<Literal, Box<dyn std::error::Error + Send + Sync>> {
            panic!("IRI-only control has no numeric token constructor")
        },
    );
    let (query, _) = SparqlParser::new()
        .parse_query_rdflib760_source_context(
            "SELECT ?x WHERE { VALUES ?x { <urn:x> } }",
            constructor,
            true,
        )
        .unwrap();
    let calls = Calls::default();
    let evaluator = QueryEvaluator::new();
    let result = evaluator.prepare(&query).execute(Dataset(calls.clone()));
    let Err(error) = result else {
        panic!("default evaluator accepted source annotation");
    };
    assert!(
        error
            .to_string()
            .contains("source context query integration")
    );
    assert_eq!(calls.internalize.get(), 0);
    assert_eq!(calls.externalize.get(), 0);
    assert_eq!(calls.scans.get(), 0);
}
