//! Frozen original calendar Duration contexts, with every group member received.
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
        crate::private_test_fixtures::text_ref!("release-inputs-03/vendor/spareval/src/eval/fixtures/source_calendar_context_query_cases_01.json"),
        "bd8ba93b4f51de1472e1536c985ef9ca203a8bad84d3b66fe425adedd3b4102c",
    ),
    (
        crate::private_test_fixtures::text_ref!("release-inputs-03/vendor/spareval/src/eval/fixtures/source_calendar_context_query_cases_02.json"),
        "c607228b53f88fe0c253fc7a73d91543400999d23d6189c857c76ff52942d8a4",
    ),
    (
        crate::private_test_fixtures::text_ref!("release-inputs-03/vendor/spareval/src/eval/fixtures/source_calendar_context_query_cases_03.json"),
        "b35d734508dc2bf4d5c01121ff275e376eeeb4a96cf3441ba57acca7745dd7bd",
    ),
    (
        crate::private_test_fixtures::text_ref!("release-inputs-03/vendor/spareval/src/eval/fixtures/source_calendar_context_query_cases_04.json"),
        "19b6a53e462d263b728cf66273e0f9eb47d568920aabda23b0f06df255d2d615",
    ),
    (
        crate::private_test_fixtures::text_ref!("release-inputs-03/vendor/spareval/src/eval/fixtures/source_calendar_context_query_cases_05.json"),
        "e856b7aff66a19bbcaa4919b353ef3a9d0abf6113f7bf94012367038df414fb0",
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
fn receive_group(first: usize, count: usize) {
    assert_eq!(
        fixtures()
            .iter()
            .map(|v| v["graph_queries"].as_array().unwrap().len())
            .sum::<usize>(),
        711
    );
    let mut failed = Vec::new();
    for ordinal in first..first + count {
        let id = format!("CC{ordinal:03}");
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
        let (root, case) = members[0];
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            source_function_registry_receiving::receive_case_with_calendar_duration(
                case,
                &root["data_inputs"],
            );
        }));
        writeln!(
            std::io::stdout().lock(),
            "OE_CALENDAR_CONTEXT_CASE {id} {}",
            if result.is_ok() { "ok" } else { "failed" }
        )
        .unwrap();
        if result.is_err() {
            failed.push(id);
        }
    }
    assert!(
        failed.is_empty(),
        "original calendar duration cases failed: {failed:?}"
    );
}
#[test]
fn source_calendar_context_construct_default_direct_false() {
    receive_group(1, 13);
}
#[test]
fn source_calendar_context_construct_default_direct_true() {
    receive_group(14, 13);
}
#[test]
fn source_calendar_context_construct_default_query() {
    receive_group(27, 13);
}
#[test]
fn source_calendar_context_relation_default() {
    receive_group(40, 18);
}
#[test]
fn source_calendar_context_arithmetic_default() {
    receive_group(58, 18);
}
#[test]
fn source_calendar_context_construct_precision_one_direct_false() {
    receive_group(76, 13);
}
#[test]
fn source_calendar_context_construct_precision_one_direct_true() {
    receive_group(89, 13);
}
#[test]
fn source_calendar_context_construct_precision_one_query() {
    receive_group(102, 13);
}
#[test]
fn source_calendar_context_relation_precision_one() {
    receive_group(115, 18);
}
#[test]
fn source_calendar_context_arithmetic_precision_one() {
    receive_group(133, 18);
}
#[test]
fn source_calendar_context_construct_precision_three_direct_false() {
    receive_group(151, 13);
}
#[test]
fn source_calendar_context_construct_precision_three_direct_true() {
    receive_group(164, 13);
}
#[test]
fn source_calendar_context_construct_precision_three_query() {
    receive_group(177, 13);
}
#[test]
fn source_calendar_context_relation_precision_three() {
    receive_group(190, 18);
}
#[test]
fn source_calendar_context_arithmetic_precision_three() {
    receive_group(208, 18);
}
#[test]
fn source_calendar_context_construct_precision_three_untrapped_direct_false() {
    receive_group(226, 13);
}
#[test]
fn source_calendar_context_construct_precision_three_untrapped_direct_true() {
    receive_group(239, 13);
}
#[test]
fn source_calendar_context_construct_precision_three_untrapped_query() {
    receive_group(252, 13);
}
#[test]
fn source_calendar_context_relation_precision_three_untrapped() {
    receive_group(265, 18);
}
#[test]
fn source_calendar_context_arithmetic_precision_three_untrapped() {
    receive_group(283, 18);
}
#[test]
fn source_calendar_context_construct_inexact_trap_direct_false() {
    receive_group(301, 13);
}
#[test]
fn source_calendar_context_construct_inexact_trap_direct_true() {
    receive_group(314, 13);
}
#[test]
fn source_calendar_context_construct_inexact_trap_query() {
    receive_group(327, 13);
}
#[test]
fn source_calendar_context_relation_inexact_trap() {
    receive_group(340, 18);
}
#[test]
fn source_calendar_context_arithmetic_inexact_trap() {
    receive_group(358, 18);
}
#[test]
fn source_calendar_context_construct_rounded_trap_direct_false() {
    receive_group(376, 13);
}
#[test]
fn source_calendar_context_construct_rounded_trap_direct_true() {
    receive_group(389, 13);
}
#[test]
fn source_calendar_context_construct_rounded_trap_query() {
    receive_group(402, 13);
}
#[test]
fn source_calendar_context_relation_rounded_trap() {
    receive_group(415, 18);
}
#[test]
fn source_calendar_context_arithmetic_rounded_trap() {
    receive_group(433, 18);
}
#[test]
fn source_calendar_context_construct_bounded_exponents_direct_false() {
    receive_group(451, 13);
}
#[test]
fn source_calendar_context_construct_bounded_exponents_direct_true() {
    receive_group(464, 13);
}
#[test]
fn source_calendar_context_construct_bounded_exponents_query() {
    receive_group(477, 13);
}
#[test]
fn source_calendar_context_relation_bounded_exponents() {
    receive_group(490, 18);
}
#[test]
fn source_calendar_context_arithmetic_bounded_exponents() {
    receive_group(508, 18);
}
#[test]
fn source_calendar_context_construct_bounded_untrapped_direct_false() {
    receive_group(526, 13);
}
#[test]
fn source_calendar_context_construct_bounded_untrapped_direct_true() {
    receive_group(539, 13);
}
#[test]
fn source_calendar_context_construct_bounded_untrapped_query() {
    receive_group(552, 13);
}
#[test]
fn source_calendar_context_relation_bounded_untrapped() {
    receive_group(565, 18);
}
#[test]
fn source_calendar_context_arithmetic_bounded_untrapped() {
    receive_group(583, 18);
}
#[test]
fn source_calendar_context_construct_subnormal_trap_direct_false() {
    receive_group(601, 13);
}
#[test]
fn source_calendar_context_construct_subnormal_trap_direct_true() {
    receive_group(614, 13);
}
#[test]
fn source_calendar_context_construct_subnormal_trap_query() {
    receive_group(627, 13);
}
#[test]
fn source_calendar_context_relation_subnormal_trap() {
    receive_group(640, 18);
}
#[test]
fn source_calendar_context_arithmetic_subnormal_trap() {
    receive_group(658, 18);
}
#[test]
fn source_calendar_context_range_direct_false() {
    receive_group(676, 12);
}
#[test]
fn source_calendar_context_range_direct_true() {
    receive_group(688, 12);
}
#[test]
fn source_calendar_context_range_query() {
    receive_group(700, 12);
}

#[test]
fn source_calendar_context_cache_isolation() {
    use crate::numeric::{LiteralSourceState, NumericInitialValue};
    use semantica_numeric::{DecimalContext, DecimalSignals};
    let decimal = DecimalContext {
        precision: 3,
        emin: (-2).into(),
        emax: 2.into(),
        traps: DecimalSignals::EMPTY,
        ..DecimalContext::default()
    };
    let low = Rc::new(
        Rc::try_unwrap(source_bgp_receiving::session_with_decimal(decimal))
            .unwrap()
            .with_source_calendar_duration_v1()
            .unwrap(),
    );
    let input = NumericInitialValue::SourceDirectString {
        lexical: "P123456789012345678901234567890Y".into(),
        datatype: Some(oxrdf::vocab::xsd::DURATION.into_owned()),
        language: None,
        normalize: false,
    };
    let value = low.construct_source_graph_initial(input).unwrap();
    let flags = low.flags();
    assert_ne!(flags, DecimalSignals::EMPTY);
    let other = Rc::new(
        Rc::try_unwrap(source_bgp_receiving::session())
            .unwrap()
            .with_source_calendar_duration_v1()
            .unwrap(),
    );
    assert_eq!(
        other
            .source_relational_equal(
                &SolutionValue::Rdf(value.clone()),
                &SolutionValue::Rdf(value.clone())
            )
            .unwrap()
            .rdf()
            .unwrap(),
        &Term::Literal(oxrdf::Literal::from(true))
    );
    // Cache validation uses the construction context without adding its flags
    // to the other query. The original sticky flags also remain unchanged.
    assert_eq!(other.flags(), DecimalSignals::EMPTY);
    assert_eq!(low.flags(), flags);
    let LiteralSourceState::Literal(metadata) = value.literal_source() else {
        panic!("source literal")
    };
    let forged = RawExpressionValue::from_import(
        oxrdf::Literal::new_typed_literal("P2Y", oxrdf::vocab::xsd::DURATION).into(),
    );
    assert!(forged.with_source_metadata(metadata.clone()).is_err());
    assert!(
        source_bgp_receiving::session()
            .source_relational_equal(
                &SolutionValue::Rdf(value.clone()),
                &SolutionValue::Rdf(value)
            )
            .is_err()
    );
}
#[test]
fn source_calendar_context_range_and_flags() {
    use semantica_numeric::{DecimalContext, DecimalSignal, DecimalSignals};
    use semantica_temporal::{DurationContext, DurationError};
    let decimal = DecimalContext::default();
    let context = DurationContext::new(&decimal);
    let duration = context.parse("P1Y").unwrap().unwrap();
    let error = context
        .apply_to_datetime(
            semantica_temporal::parse_datetime("9999-12-31T23:59:59.999999").unwrap(),
            &duration,
            false,
        )
        .unwrap_err();
    assert_eq!(
        error.downcast_ref::<DurationError>(),
        Some(&DurationError::YearRange(10000))
    );
    let reached =
        DecimalSignals::from(DecimalSignal::Inexact) | DecimalSignals::from(DecimalSignal::Rounded);
    assert_eq!(context.signals(), reached);
    // Successful recovery does not erase the reached context flags.
    let zero = context.parse("PT0S").unwrap().unwrap();
    assert!(!zero.python_truth());
    assert_eq!(context.signals(), reached);
    let clean = DurationContext::new(&decimal);
    let micro = clean.parse("PT0.000001S").unwrap().unwrap();
    let error = clean
        .apply_to_datetime(
            semantica_temporal::parse_datetime("9999-12-31T23:59:59.999999").unwrap(),
            &micro,
            false,
        )
        .unwrap_err();
    assert_eq!(
        error.downcast_ref::<DurationError>(),
        Some(&DurationError::Overflow("date value out of range"))
    );
    assert_eq!(clean.signals(), DecimalSignals::EMPTY);
}
