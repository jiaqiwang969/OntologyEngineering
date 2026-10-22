//! Frozen original calendar Duration contexts, with every group member received.
use super::*;
use serde_json::Value;
use std::io::Write;
use std::sync::OnceLock;
fn private_fixtures() -> &'static [(&'static str, &'static str)] {
    static TEXTS: std::sync::OnceLock<Vec<(&'static str, &'static str)>> =
        std::sync::OnceLock::new();
    TEXTS.get_or_init(|| vec![
    (
        crate::private_test_fixtures::text_ref!("release-inputs-03/vendor/spareval/src/eval/fixtures/source_calendar_duration_query_cases_01.json"),
        "4056f7dd66cbd84f244b9a26edeb946ebb2857bdf39b358f877dcbf8e39ba6ee",
    ),
    (
        crate::private_test_fixtures::text_ref!("release-inputs-03/vendor/spareval/src/eval/fixtures/source_calendar_duration_query_cases_02.json"),
        "2742b0757eb82f056944e916a38fb0cb32ef446f7e04a63028368df9c204bd6c",
    ),
    (
        crate::private_test_fixtures::text_ref!("release-inputs-03/vendor/spareval/src/eval/fixtures/source_calendar_duration_query_cases_03.json"),
        "deafd6f3fdd5a7d8960db6e66051e98de7a66057afa7cdca47a3f70b1e530bea",
    ),
    (
        crate::private_test_fixtures::text_ref!("release-inputs-03/vendor/spareval/src/eval/fixtures/source_calendar_duration_query_cases_04.json"),
        "647e4d4d35c33ec2091420a3ed5feb62c02f768e801b41fb0f71930ec4f79cd1",
    ),
    (
        crate::private_test_fixtures::text_ref!("release-inputs-03/vendor/spareval/src/eval/fixtures/source_calendar_duration_query_cases_05.json"),
        "55d1719ce285ee4ee08232641e2224ffa66c779626e6ddd5eb164f177804b474",
    ),
    (
        crate::private_test_fixtures::text_ref!("release-inputs-03/vendor/spareval/src/eval/fixtures/source_calendar_duration_query_cases_06.json"),
        "9da3e69df540363d8cada954c3e4f95cc0eccbda4ee29b2ca8f6ea97797f4bec",
    ),
    (
        crate::private_test_fixtures::text_ref!("release-inputs-03/vendor/spareval/src/eval/fixtures/source_calendar_duration_query_cases_07.json"),
        "e832941428d105f107d2b156b30d86bf0f2ab11992fffaa93c087c880327fc74",
    ),
]).as_slice()
}
// This independent digest still names the exact original JSON bytes. The
// envelope is storage only; neither identity may be supplied by its payload.
fn decode_original_fixture(bytes: &str, expected: &str) -> Result<Value, String> {
    if bytes.len() > 1024 * 1024 {
        return Err("packed_fixture_budget".into());
    }
    let envelope: Value = serde_json::from_str(bytes).map_err(|_| "packed_fixture_json")?;
    if envelope["decoded_sha256"] != expected || envelope["original_fixture_sha256"] != expected {
        return Err("calendar_fixture_original_identity".into());
    }
    super::source_temporal_operands_receiving::decode_fixture(bytes)
}
fn fixtures() -> &'static Vec<Value> {
    static DATA: OnceLock<Vec<Value>> = OnceLock::new();
    DATA.get_or_init(|| {
        private_fixtures()
            .iter()
            .map(|(bytes, digest)| {
                let root = decode_original_fixture(bytes, digest).unwrap();
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
#[test]
fn source_calendar_duration_packed_complete_original_membership() {
    let mut ids = std::collections::BTreeSet::new();
    assert_eq!(private_fixtures().len(), 7);
    for root in fixtures() {
        for (case, id) in root["graph_queries"]
            .as_array()
            .unwrap()
            .iter()
            .zip(root["original_case_ids"].as_array().unwrap())
        {
            assert_eq!(case["input"]["id"], *id);
            assert!(ids.insert(id.as_str().unwrap().to_owned()));
        }
    }
    assert_eq!(ids, (1..=834).map(|i| format!("CD{i:03}")).collect());
}
#[test]
fn source_calendar_duration_packed_independent_identity_rejects_substitution() {
    let (bytes, expected) = private_fixtures()[0];
    // A valid envelope for another actual fixture passes the shared codec but
    // must not satisfy this fixture's independent original identity.
    assert!(
        super::source_temporal_operands_receiving::decode_fixture(private_fixtures()[1].0).is_ok()
    );
    assert_eq!(
        decode_original_fixture(private_fixtures()[1].0, expected).unwrap_err(),
        "calendar_fixture_original_identity"
    );
    for field in ["decoded_sha256", "original_fixture_sha256"] {
        let mut envelope: Value = serde_json::from_str(bytes).unwrap();
        envelope[field] = Value::String("0".repeat(64));
        assert_eq!(
            decode_original_fixture(&envelope.to_string(), expected).unwrap_err(),
            "calendar_fixture_original_identity"
        );
    }
    assert!(decode_original_fixture(bytes, expected).is_ok());
}
fn receive_group(first: usize, count: usize) {
    assert_eq!(
        fixtures()
            .iter()
            .map(|v| v["graph_queries"].as_array().unwrap().len())
            .sum::<usize>(),
        834
    );
    let mut failed = Vec::new();
    for ordinal in first..first + count {
        let id = format!("CD{ordinal:03}");
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
            "OE_CALENDAR_DURATION_CASE {id} {}",
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
fn source_calendar_duration_construct_duration_direct_false() {
    receive_group(1, 32);
}
#[test]
fn source_calendar_duration_construct_duration_direct_true() {
    receive_group(33, 32);
}
#[test]
fn source_calendar_duration_construct_duration_query() {
    receive_group(65, 32);
}
#[test]
fn source_calendar_duration_construct_yearmonthduration_direct_false() {
    receive_group(97, 32);
}
#[test]
fn source_calendar_duration_construct_yearmonthduration_direct_true() {
    receive_group(129, 32);
}
#[test]
fn source_calendar_duration_construct_yearmonthduration_query() {
    receive_group(161, 32);
}
#[test]
fn source_calendar_duration_construct_daytimeduration_direct_false() {
    receive_group(193, 32);
}
#[test]
fn source_calendar_duration_construct_daytimeduration_direct_true() {
    receive_group(225, 32);
}
#[test]
fn source_calendar_duration_construct_daytimeduration_query() {
    receive_group(257, 32);
}
#[test]
fn source_calendar_duration_relation_01() {
    receive_group(289, 18);
}
#[test]
fn source_calendar_duration_relation_02() {
    receive_group(307, 18);
}
#[test]
fn source_calendar_duration_relation_03() {
    receive_group(325, 18);
}
#[test]
fn source_calendar_duration_relation_04() {
    receive_group(343, 18);
}
#[test]
fn source_calendar_duration_relation_05() {
    receive_group(361, 18);
}
#[test]
fn source_calendar_duration_relation_06() {
    receive_group(379, 18);
}
#[test]
fn source_calendar_duration_relation_07() {
    receive_group(397, 18);
}
#[test]
fn source_calendar_duration_relation_08() {
    receive_group(415, 18);
}
#[test]
fn source_calendar_duration_relation_09() {
    receive_group(433, 18);
}
#[test]
fn source_calendar_duration_relation_10() {
    receive_group(451, 18);
}
#[test]
fn source_calendar_duration_relation_11() {
    receive_group(469, 18);
}
#[test]
fn source_calendar_duration_relation_12() {
    receive_group(487, 18);
}
#[test]
fn source_calendar_duration_order_01() {
    receive_group(505, 3);
}
#[test]
fn source_calendar_duration_order_02() {
    receive_group(508, 3);
}
#[test]
fn source_calendar_duration_order_03() {
    receive_group(511, 3);
}
#[test]
fn source_calendar_duration_order_04() {
    receive_group(514, 3);
}
#[test]
fn source_calendar_duration_order_05() {
    receive_group(517, 3);
}
#[test]
fn source_calendar_duration_order_06() {
    receive_group(520, 3);
}
#[test]
fn source_calendar_duration_arithmetic_01() {
    receive_group(523, 12);
}
#[test]
fn source_calendar_duration_arithmetic_02() {
    receive_group(535, 12);
}
#[test]
fn source_calendar_duration_arithmetic_03() {
    receive_group(547, 12);
}
#[test]
fn source_calendar_duration_arithmetic_04() {
    receive_group(559, 12);
}
#[test]
fn source_calendar_duration_arithmetic_05() {
    receive_group(571, 12);
}
#[test]
fn source_calendar_duration_arithmetic_06() {
    receive_group(583, 12);
}
#[test]
fn source_calendar_duration_arithmetic_07() {
    receive_group(595, 12);
}
#[test]
fn source_calendar_duration_arithmetic_08() {
    receive_group(607, 12);
}
#[test]
fn source_calendar_duration_arithmetic_09() {
    receive_group(619, 12);
}
#[test]
fn source_calendar_duration_arithmetic_10() {
    receive_group(631, 12);
}
#[test]
fn source_calendar_duration_arithmetic_11() {
    receive_group(643, 12);
}
#[test]
fn source_calendar_duration_unary_duration_direct_false() {
    receive_group(655, 12);
}
#[test]
fn source_calendar_duration_unary_duration_direct_true() {
    receive_group(667, 12);
}
#[test]
fn source_calendar_duration_unary_duration_query() {
    receive_group(679, 12);
}
#[test]
fn source_calendar_duration_unary_yearmonthduration_direct_false() {
    receive_group(691, 12);
}
#[test]
fn source_calendar_duration_unary_yearmonthduration_direct_true() {
    receive_group(703, 12);
}
#[test]
fn source_calendar_duration_unary_yearmonthduration_query() {
    receive_group(715, 12);
}
#[test]
fn source_calendar_duration_unary_daytimeduration_direct_false() {
    receive_group(727, 12);
}
#[test]
fn source_calendar_duration_unary_daytimeduration_direct_true() {
    receive_group(739, 12);
}
#[test]
fn source_calendar_duration_unary_daytimeduration_query() {
    receive_group(751, 12);
}
#[test]
fn source_calendar_duration_consumer_direct_false() {
    receive_group(763, 24);
}
#[test]
fn source_calendar_duration_consumer_direct_true() {
    receive_group(787, 24);
}
#[test]
fn source_calendar_duration_consumer_query() {
    receive_group(811, 24);
}

fn calendar_session() -> Rc<QueryNumericSession> {
    Rc::new(
        Rc::try_unwrap(source_bgp_receiving::session())
            .ok()
            .unwrap()
            .with_source_calendar_duration_v1()
            .unwrap(),
    )
}
fn duration_input(text: &str) -> crate::numeric::NumericInitialValue {
    crate::numeric::NumericInitialValue::SourceDirectString {
        lexical: text.into(),
        datatype: Some(oxrdf::vocab::xsd::DURATION.into_owned()),
        language: None,
        normalize: false,
    }
}
#[test]
fn source_calendar_duration_closed_entries() {
    use crate::numeric::{NumericInitialBinding, NumericInitialValue, RawNumericFault};
    let old = source_bgp_receiving::session();
    assert!(!old.admits_source_calendar_duration());
    assert!(
        old.construct_source_graph_initial(duration_input("P1Y"))
            .is_err()
    );
    let calendar = calendar_session();
    assert!(calendar.construct_initial(duration_input("P1Y")).is_err());
    assert!(
        calendar
            .construct_source_graph_initial(duration_input(&"P".repeat(10_001)))
            .is_err()
    );
    // Source application construction is before parsing, and occurs once.
    let input = NumericInitialBinding {
        variable: oxrdf::Variable::new("a").unwrap(),
        value: NumericInitialValue::SourceDirectString {
            lexical: "-P1Y2D".into(),
            datatype: Some(oxrdf::vocab::xsd::DURATION.into_owned()),
            language: None,
            normalize: true,
        },
    };
    let result = numeric_query::prepare_source_graph_numeric_query(
        "this is not SPARQL",
        source_bgp_receiving::dataset(&calendar, None),
        Rc::clone(&calendar),
        vec![input],
    );
    assert!(matches!(
        result,
        Err(
            numeric_query::NumericPrepareError::InitialLiteralConstruction(
                RawNumericFault::SourceLanguageException(_)
            )
        )
    ));
    assert!(
        old.construct_source_graph_initial(duration_input("P1Y"))
            .is_err()
    );
}
#[test]
fn source_calendar_duration_unknown_origin() {
    use crate::numeric::{LiteralSourceState, SourceOrderKey};
    let session = calendar_session();
    let known = session
        .construct_source_graph_initial(duration_input("P1Y"))
        .unwrap();
    let imported = RawExpressionValue::from_import(
        oxrdf::Literal::new_typed_literal("P1Y", oxrdf::vocab::xsd::DURATION).into(),
    );
    assert!(
        session
            .source_relational_equal(
                &SolutionValue::Rdf(known.clone()),
                &SolutionValue::Rdf(imported.clone())
            )
            .is_err()
    );
    assert!(
        session
            .source_order_key_less(
                &SourceOrderKey::Rdf(known.clone()),
                &SourceOrderKey::Rdf(imported.clone())
            )
            .is_err()
    );
    let LiteralSourceState::Literal(metadata) = known.literal_source() else {
        panic!("constructed cache")
    };
    let forged = RawExpressionValue::from_import(
        oxrdf::Literal::new_typed_literal("P2Y", oxrdf::vocab::xsd::DURATION).into(),
    );
    assert!(forged.with_source_metadata(metadata.clone()).is_err());
    let old = source_bgp_receiving::session();
    assert!(
        old.source_relational_equal(
            &SolutionValue::Rdf(known.clone()),
            &SolutionValue::Rdf(known)
        )
        .is_err()
    );
}
#[test]
fn source_calendar_duration_calendar_and_fixed_are_distinct() {
    use semantica_temporal::{DurationContext, DurationValue};
    let decimal = semantica_numeric::DecimalContext::default();
    let context = DurationContext::new(&decimal);
    let year = context.parse("P1Y").unwrap().unwrap();
    let days = context.parse("P365D").unwrap().unwrap();
    assert!(matches!(year, DurationValue::Calendar { .. }));
    assert!(matches!(days, DurationValue::Fixed(_)));
    assert!(!context.value_eq(&year, &days).unwrap());
    let zero = context.parse("P0Y0M").unwrap().unwrap();
    assert!(!zero.python_truth());
    let fractional = context.parse("P0.10Y").unwrap().unwrap();
    assert!(fractional.python_truth());
    assert_eq!(context.format(&fractional).unwrap(), "P0D");
    assert!(!context.value_eq(&fractional, &zero).unwrap());
    assert!(
        context
            .apply_to_date(
                semantica_temporal::parse_date("2020-01-31").unwrap(),
                &fractional,
                false
            )
            .is_err()
    );
}
