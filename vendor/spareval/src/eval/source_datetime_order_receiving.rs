//! Original datetime comparison and total ordering; all group members execute.
use super::*;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::Write;
fn private_fixtures() -> [(&'static str, &'static str); 5] {
    [
        (
            crate::private_test_fixtures::text_ref!(
                "release-inputs-03/vendor/spareval/src/eval/fixtures/source_datetime_order_query_cases_01.json"
            ),
            "e153e7383a7f3780b80769ce2754a787b92d9c7bf838b9c6c6ed3afb890da668",
        ),
        (
            crate::private_test_fixtures::text_ref!(
                "release-inputs-03/vendor/spareval/src/eval/fixtures/source_datetime_order_query_cases_02.json"
            ),
            "363f25b85ca5dd72477d9f21cd8f1a81acefb17c491d4e1923a4631aaca94152",
        ),
        (
            crate::private_test_fixtures::text_ref!(
                "release-inputs-03/vendor/spareval/src/eval/fixtures/source_datetime_order_query_cases_03.json"
            ),
            "b80c5a55268d405ed6b6fd05d38447009b2ed553467a62192c0aa9f548a2c41b",
        ),
        (
            crate::private_test_fixtures::text_ref!(
                "release-inputs-03/vendor/spareval/src/eval/fixtures/source_datetime_order_query_cases_04.json"
            ),
            "5c1d21401bca9b5da76dc553fbdc0a66b99eafb3362a9b730d14a4c20ba7b97a",
        ),
        (
            crate::private_test_fixtures::text_ref!(
                "release-inputs-03/vendor/spareval/src/eval/fixtures/source_datetime_order_query_cases_05.json"
            ),
            "d6b9e4255771bc048b592861df5ab5ad8d75bd7ce76434f9e04ab43cf1993a47",
        ),
    ]
}
fn receive_group(shard: usize, first: usize, count: usize) {
    let (bytes, digest) = private_fixtures()[shard];
    assert_eq!(format!("{:x}", Sha256::digest(bytes.as_bytes())), digest);
    let root: Value = serde_json::from_str(bytes).unwrap();
    assert_eq!(root["candidate_results_used"], false);
    let cases = root["graph_queries"].as_array().unwrap();
    assert_eq!(cases.len(), if shard == 4 { 36 } else { 72 });
    let mut failed = Vec::new();
    for ordinal in first..first + count {
        let id = format!("DO{ordinal:03}");
        let members: Vec<_> = cases.iter().filter(|c| c["input"]["id"] == id).collect();
        assert_eq!(members.len(), 1, "original case membership {id}");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            source_function_registry_receiving::receive_case(members[0], &root["data_inputs"]);
        }));
        // Write one atomic line even for successful tests: qualification checks
        // all IDs exactly once, so grouped tests cannot hide missing cases.
        writeln!(
            std::io::stdout().lock(),
            "OE_DATETIME_ORDER_CASE {id} {}",
            if result.is_ok() { "ok" } else { "failed" }
        )
        .unwrap();
        if result.is_err() {
            failed.push(id);
        }
    }
    assert!(
        failed.is_empty(),
        "original datetime cases failed: {failed:?}"
    );
}
#[test]
fn source_datetime_order_pair01() {
    receive_group(0, 1, 18);
}
#[test]
fn source_datetime_order_pair02() {
    receive_group(0, 19, 18);
}
#[test]
fn source_datetime_order_pair03() {
    receive_group(0, 37, 18);
}
#[test]
fn source_datetime_order_pair04() {
    receive_group(0, 55, 18);
}
#[test]
fn source_datetime_order_pair05() {
    receive_group(1, 73, 18);
}
#[test]
fn source_datetime_order_pair06() {
    receive_group(1, 91, 18);
}
#[test]
fn source_datetime_order_pair07() {
    receive_group(1, 109, 18);
}
#[test]
fn source_datetime_order_pair08() {
    receive_group(1, 127, 18);
}
#[test]
fn source_datetime_order_pair09() {
    receive_group(2, 145, 18);
}
#[test]
fn source_datetime_order_pair10() {
    receive_group(2, 163, 18);
}
#[test]
fn source_datetime_order_pair11() {
    receive_group(2, 181, 18);
}
#[test]
fn source_datetime_order_pair12() {
    receive_group(2, 199, 18);
}
#[test]
fn source_datetime_order_pair13() {
    receive_group(3, 217, 18);
}
#[test]
fn source_datetime_order_pair14() {
    receive_group(3, 235, 18);
}
#[test]
fn source_datetime_order_pair15() {
    receive_group(3, 253, 18);
}
#[test]
fn source_datetime_order_pair16() {
    receive_group(3, 271, 18);
}
#[test]
fn source_datetime_order_sequence01() {
    receive_group(4, 289, 3);
}
#[test]
fn source_datetime_order_sequence02() {
    receive_group(4, 292, 3);
}
#[test]
fn source_datetime_order_sequence03() {
    receive_group(4, 295, 3);
}
#[test]
fn source_datetime_order_sequence04() {
    receive_group(4, 298, 3);
}
#[test]
fn source_datetime_order_sequence05() {
    receive_group(4, 301, 3);
}
#[test]
fn source_datetime_order_sequence06() {
    receive_group(4, 304, 3);
}
#[test]
fn source_datetime_order_sequence07() {
    receive_group(4, 307, 3);
}
#[test]
fn source_datetime_order_sequence08() {
    receive_group(4, 310, 3);
}
#[test]
fn source_datetime_order_sequence09() {
    receive_group(4, 313, 3);
}
#[test]
fn source_datetime_order_sequence10() {
    receive_group(4, 316, 3);
}
#[test]
fn source_datetime_order_sequence11() {
    receive_group(4, 319, 3);
}
#[test]
fn source_datetime_order_sequence12() {
    receive_group(4, 322, 3);
}

#[test]
fn source_datetime_order_value_semantics() {
    let naive = semantica_temporal::parse_datetime("2025-01-01T13:00:00").unwrap();
    let aware = semantica_temporal::parse_datetime("2020-01-01T11:00:00Z").unwrap();
    assert_eq!(naive.value_cmp(aware), None);
    assert_eq!(aware.value_cmp(naive), None);
    assert!(!naive.value_eq(aware));
    let a = semantica_temporal::parse_datetime("2020-01-01T12:00:00+00:00:01.000001").unwrap();
    let b = semantica_temporal::parse_datetime("2020-01-01T12:00:00+00:00:01.000002").unwrap();
    assert!(!a.value_eq(b)); // datetime still uses the complete microsecond offset.
}
#[test]
fn source_datetime_order_unknown_origin() {
    use crate::numeric::{NumericInitialValue, SourceOrderKey};
    let session = source_bgp_receiving::session();
    let a = session
        .construct_source_graph_initial(NumericInitialValue::SourceDirectString {
            lexical: "2020-01-01T12:00:00".into(),
            datatype: Some(oxrdf::vocab::xsd::DATE_TIME.into_owned()),
            language: None,
            normalize: false,
        })
        .unwrap();
    let b = RawExpressionValue::from_import(
        oxrdf::Literal::new_typed_literal("2020-01-01T13:00:00Z", oxrdf::vocab::xsd::DATE_TIME)
            .into(),
    );
    assert!(
        session
            .source_order_key_less(
                &SourceOrderKey::Rdf(a.clone()),
                &SourceOrderKey::Rdf(b.clone())
            )
            .is_err()
    );
    assert!(
        session
            .source_relational_equal(&SolutionValue::Rdf(a), &SolutionValue::Rdf(b))
            .is_err()
    );
}
