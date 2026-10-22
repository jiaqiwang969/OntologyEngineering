//! Frozen temporal difference observations. All480 inputs retained;24 trace failures have separately bound recovered Function observations.
use super::*;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::Write;
use std::sync::OnceLock;
fn private_fixtures() -> &'static [(&'static str, &'static str)] {
    static TEXTS: std::sync::OnceLock<Vec<(&'static str, &'static str)>> =
        std::sync::OnceLock::new();
    TEXTS.get_or_init(|| vec![
    (
        crate::private_test_fixtures::text_ref!("release-inputs-03/vendor/spareval/src/eval/fixtures/source_temporal_difference_query_cases_01.json"),
        "328cb3a33e9ed3a874457ae4f404f7215b59bb23159d54aec41d13b03e661e28",
    ),
    (
        crate::private_test_fixtures::text_ref!("release-inputs-03/vendor/spareval/src/eval/fixtures/source_temporal_difference_query_cases_02.json"),
        "450aa6d17e88e1ec9f6759dd776e32e44576dfb1c60f32ab27d72f1d08a483af",
    ),
    (
        crate::private_test_fixtures::text_ref!("release-inputs-03/vendor/spareval/src/eval/fixtures/source_temporal_difference_query_cases_03.json"),
        "51fa6c4dd4668db495a7576eb2c0f285751c9087350dc7478b6b38ab5c9b40bc",
    ),
    (
        crate::private_test_fixtures::text_ref!("release-inputs-03/vendor/spareval/src/eval/fixtures/source_temporal_difference_query_cases_04.json"),
        "3046844a5c8c8a724983f08d8d33eb19d601fd0c7f2bb413dbd5be7fca3b0159",
    ),
    (
        crate::private_test_fixtures::text_ref!("release-inputs-03/vendor/spareval/src/eval/fixtures/source_temporal_difference_query_cases_05.json"),
        "e457df67040cc567b21a32ac784c4d9a4719a960a9931bb2099685fb57d18288",
    ),
    (
        crate::private_test_fixtures::text_ref!("release-inputs-03/vendor/spareval/src/eval/fixtures/source_temporal_difference_query_cases_06.json"),
        "ae1d0ff060bfbc7c223a83c3a5c206cc672797ecef6223fb7433d4ada044fdda",
    ),
]).as_slice()
}
fn fixtures() -> &'static Vec<Value> {
    static DATA: OnceLock<Vec<Value>> = OnceLock::new();
    DATA.get_or_init(|| {
        private_fixtures()
            .iter()
            .map(|(bytes, digest)| {
                assert_eq!(format!("{:x}", Sha256::digest(bytes.as_bytes())), *digest);
                let root: Value = serde_json::from_str(bytes).unwrap();
                assert_eq!(root["candidate_results_used"], false);
                assert_eq!(
                    root["graph_queries"].as_array().unwrap().len(),
                    root["original_case_ids"].as_array().unwrap().len()
                );
                root
            })
            .collect()
    })
}
fn recovered() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| {
        let bytes = crate::private_test_fixtures::text_ref!("release-inputs-03/vendor/spareval/src/eval/fixtures/source_temporal_difference_recovered_01.json");
        assert_eq!(
            format!("{:x}", Sha256::digest(bytes.as_bytes())),
            "cbcb536f509b000cdf48391e391762eae4ccbd9cc03a6565b8229636808caac8"
        );
        let root: Value = serde_json::from_str(bytes).unwrap();
        assert_eq!(root["candidate_results_used"], false);
        root
    })
}
fn receive_group(first: usize, count: usize) {
    assert_eq!(
        fixtures()
            .iter()
            .map(|v| v["graph_queries"].as_array().unwrap().len())
            .sum::<usize>(),
        480
    );
    let mut failed = Vec::new();
    for ordinal in first..first + count {
        let id = format!("TD{ordinal:03}");
        let members: Vec<_> = fixtures()
            .iter()
            .flat_map(|root| {
                root["graph_queries"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|c| c["input"]["id"] == id)
                    .map(move |c| (root, c))
            })
            .collect();
        assert_eq!(members.len(), 1, "original case membership {id}");
        let (root, original_case) = members[0];
        let case = if original_case["status"] == "observer_error" {
            let cases: Vec<_> = recovered()["graph_queries"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|c| c["input"]["id"] == id)
                .collect();
            assert_eq!(cases.len(), 1);
            assert_eq!(cases[0]["input"], original_case["input"]);
            assert_eq!(recovered()["data_inputs"], root["data_inputs"]);
            cases[0]
        } else {
            original_case
        };
        assert!(case["status"] == "observed_complete" || case["status"] == "original_error");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            source_function_registry_receiving::receive_case_with_temporal_arithmetic(
                case,
                &root["data_inputs"],
            );
        }));
        writeln!(
            std::io::stdout().lock(),
            "OE_TEMPORAL_DIFFERENCE_CASE {id} {}",
            if result.is_ok() { "ok" } else { "failed" }
        )
        .unwrap();
        if result.is_err() {
            failed.push(id);
        }
    }
    assert!(
        failed.is_empty(),
        "original temporal difference cases failed: {failed:?}"
    );
}
#[test]
fn source_temporal_difference_difference_datetime_direct_false_observed() {
    receive_group(1, 52);
}
#[test]
fn source_temporal_difference_difference_datetime_direct_true_observed() {
    receive_group(53, 52);
}
#[test]
fn source_temporal_difference_difference_datetime_query_observed() {
    receive_group(105, 52);
}
#[test]
fn source_temporal_difference_difference_date_direct_false_observed() {
    receive_group(157, 22);
}
#[test]
fn source_temporal_difference_difference_date_direct_true_observed() {
    receive_group(179, 22);
}
#[test]
fn source_temporal_difference_difference_date_query_observed() {
    receive_group(201, 22);
}
#[test]
fn source_temporal_difference_difference_time_direct_false_observed() {
    receive_group(223, 18);
}
#[test]
fn source_temporal_difference_difference_time_direct_true_observed() {
    receive_group(241, 18);
}
#[test]
fn source_temporal_difference_difference_time_query_observed() {
    receive_group(259, 18);
}
#[test]
fn source_temporal_difference_time_arithmetic_direct_false_observed() {
    receive_group(277, 48);
}
#[test]
fn source_temporal_difference_time_arithmetic_direct_true_observed() {
    receive_group(325, 48);
}
#[test]
fn source_temporal_difference_time_arithmetic_query_observed() {
    receive_group(373, 48);
}
#[test]
fn source_temporal_difference_chains_direct_false_observed() {
    receive_group(421, 15);
}
#[test]
fn source_temporal_difference_chains_direct_true_observed() {
    receive_group(436, 15);
}
#[test]
fn source_temporal_difference_chains_query_observed() {
    receive_group(451, 15);
}
#[test]
fn source_temporal_difference_consumers_direct_false_observed() {
    receive_group(466, 5);
}
#[test]
fn source_temporal_difference_consumers_direct_true_observed() {
    receive_group(471, 5);
}
#[test]
fn source_temporal_difference_consumers_query_observed() {
    receive_group(476, 5);
}

#[test]
fn source_temporal_difference_recursion_observation_recovered() {
    let missing: Vec<_> = fixtures()
        .iter()
        .flat_map(|v| v["graph_queries"].as_array().unwrap())
        .filter(|v| v["status"] == "observer_error")
        .map(|v| v["input"]["id"].clone())
        .collect();
    assert_eq!(missing.len(), 24);
    assert_eq!(serde_json::json!(missing), recovered()["original_case_ids"]);
    let cases = recovered()["graph_queries"].as_array().unwrap();
    assert_eq!(cases.len(), 24);
    for case in cases {
        assert_eq!(case["status"], "original_error");
        assert_eq!(case["error"]["class_name"], "RecursionError");
        assert_eq!(case["trace"].as_array().unwrap().len(), 3);
        assert_eq!(case["trace"][1]["exception"], case["error"]);
        assert!(case.get("observer_error").is_none());
    }
}

fn temporal_input(session: &QueryNumericSession, text: &str) -> RawExpressionValue {
    session
        .construct_source_graph_initial(crate::numeric::NumericInitialValue::SourceDirectString {
            lexical: text.into(),
            datatype: Some(oxrdf::vocab::xsd::DATE_TIME.into_owned()),
            language: None,
            normalize: false,
        })
        .unwrap()
}
fn temporal_subtract(
    session: &QueryNumericSession,
    a: &RawExpressionValue,
    b: &RawExpressionValue,
) -> Result<SolutionValue<RawExpressionValue>, crate::numeric::NumericEvaluationError> {
    session.evaluate_chain_value(
        crate::numeric::NumericChainKind::Additive,
        SolutionValue::Rdf(a.clone()),
        vec![(
            crate::numeric::NumericBinaryOp::Subtract,
            SolutionValue::Rdf(b.clone()),
        )],
    )
}
#[test]
fn source_temporal_difference_admission_and_budget() {
    use crate::numeric::{
        NumericEvaluationError, QueryContextSemantics, QueryGroupSemantics,
        QueryNumericCapabilities, RawNumericFault, SourceHashPolicy, SourceHashRuntime,
    };
    use semantica_numeric::{DecimalContext, NumericFault, NumericLimits};
    let session = Rc::try_unwrap(source_bgp_receiving::session())
        .unwrap()
        .with_source_calendar_duration_v1()
        .unwrap();
    let a = temporal_input(&session, "2020-01-02T00:00:00");
    let b = temporal_input(&session, "2020-01-01T00:00:00");
    assert!(temporal_subtract(&source_bgp_receiving::session(), &a, &b).is_err());
    let limited = QueryNumericSession::new_with_context_semantics(
        DecimalContext::default(),
        NumericLimits {
            max_output_characters: 2,
            ..NumericLimits::default()
        },
        None,
        true,
        QueryNumericCapabilities::MixedBinary64V1,
        QueryGroupSemantics::SourceAggregateV1,
        QueryLiteralSemantics::SourceLiteralV1,
        QueryContextSemantics::SourceContextV1,
        Some(Rc::new(
            SourceHashRuntime::new(SourceHashPolicy::Seeded(0)).unwrap(),
        )),
    )
    .unwrap()
    .with_source_calendar_duration_v1()
    .unwrap();
    assert!(matches!(
        temporal_subtract(&limited, &a, &b),
        Err(NumericEvaluationError::Fatal(RawNumericFault::Core(
            NumericFault::ResourceLimit {
                operation: "output_characters"
            }
        )))
    ));
    let imported = RawExpressionValue::from_import(
        oxrdf::Literal::new_typed_literal("2020-01-01T00:00:00", oxrdf::vocab::xsd::DATE_TIME)
            .into(),
    );
    assert!(temporal_subtract(&session, &a, &imported).is_err());
    let SolutionValue::Rdf(value) = temporal_subtract(&session, &a, &b).unwrap() else {
        panic!("duration");
    };
    assert_eq!(
        value.rdf().unwrap(),
        &Term::Literal(oxrdf::Literal::new_typed_literal(
            "P1D",
            oxrdf::vocab::xsd::DURATION
        ))
    );
}
#[test]
fn source_temporal_difference_cache_and_recovery() {
    use crate::numeric::{
        LiteralSourceState, NumericEvaluationError, NumericInitialValue, RawNumericFault,
        SourceFact, SourceLiteralCacheKind,
    };
    use semantica_numeric::{DecimalContext, DecimalSignals};
    let decimal = DecimalContext {
        precision: 3,
        emin: (-2).into(),
        emax: 2.into(),
        traps: DecimalSignals::EMPTY,
        ..DecimalContext::default()
    };
    let session = Rc::try_unwrap(source_bgp_receiving::session_with_decimal(decimal))
        .unwrap()
        .with_source_calendar_duration_v1()
        .unwrap();
    // Reach actual constructor signals, then verify difference and failure
    // recovery retain them without doing fresh Decimal arithmetic.
    session
        .construct_source_graph_initial(NumericInitialValue::SourceDirectString {
            lexical: "P123456789012345678901234567890Y".into(),
            datatype: Some(oxrdf::vocab::xsd::DURATION.into_owned()),
            language: None,
            normalize: false,
        })
        .unwrap();
    let flags = session.flags();
    assert_ne!(flags, DecimalSignals::EMPTY);
    let a = temporal_input(&session, "2020-01-01T00:00:00.000001");
    let b = temporal_input(&session, "2020-01-01T00:00:00");
    let bad = temporal_input(&session, "bad");
    let Err(NumericEvaluationError::Fatal(RawNumericFault::SourceLanguageException(error))) =
        temporal_subtract(&session, &bad, &b)
    else {
        panic!("source recursion failure");
    };
    assert_eq!(error.kind.class_name(), "RecursionError");
    assert_eq!(session.flags(), flags);
    let SolutionValue::Rdf(result) = temporal_subtract(&session, &a, &b).unwrap() else {
        panic!("duration");
    };
    let LiteralSourceState::Literal(meta) = result.literal_source() else {
        panic!("metadata");
    };
    assert_eq!(meta.ill_typed, SourceFact::Known(None));
    let SourceFact::Known(SourceLiteralCacheKind::Duration(cache)) = &meta.cached_value_kind else {
        panic!("exact result cache");
    };
    assert_eq!(
        cache.value,
        Some(semantica_temporal::DurationValue::Fixed(
            semantica_temporal::Timedelta::from_micros(1).unwrap()
        ))
    );
    assert_eq!(session.flags(), flags);
    let forged = RawExpressionValue::from_import(
        oxrdf::Literal::new_typed_literal("P1D", oxrdf::vocab::xsd::DURATION).into(),
    );
    assert!(forged.with_source_metadata(meta.clone()).is_err());
}
