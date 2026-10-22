//! Complete frozen mixed temporal operand observations; same evaluator owner.
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
        crate::private_test_fixtures::text_ref!("release-inputs-03/vendor/spareval/src/eval/fixtures/source_temporal_domain_query_cases_01.json"),
        "3a2720e50940e2c8bb0571e613d66913badeae1e4cb91d0553731c35e2ad7f0d",
    ),
    (
        crate::private_test_fixtures::text_ref!("release-inputs-03/vendor/spareval/src/eval/fixtures/source_temporal_domain_query_cases_02.json"),
        "7f69393cf580d96a96909c69c79f0e9901fbf7ee158a7f1ff4098503e86cff87",
    ),
    (
        crate::private_test_fixtures::text_ref!("release-inputs-03/vendor/spareval/src/eval/fixtures/source_temporal_domain_query_cases_03.json"),
        "f3f826e58671e6485dcf1934010f42b038003e93b0685112a6c5eaafe9933bd7",
    ),
    (
        crate::private_test_fixtures::text_ref!("release-inputs-03/vendor/spareval/src/eval/fixtures/source_temporal_domain_query_cases_04.json"),
        "ef21b91e9f1e2db64239b8ec5742c18957c22db113a8ad10e2c1ede2c7ccc0e2",
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
                let root = source_temporal_operands_receiving::decode_fixture(bytes).unwrap();
                assert_eq!(root["candidate_results_used"], false);
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
        792
    );
    let mut failed = Vec::new();
    for ordinal in first..first + count {
        let id = format!("TM{ordinal:03}");
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
        assert_eq!(members.len(), 1);
        let (root, case) = members[0];
        assert!(case["status"] == "observed_complete" || case["status"] == "original_error");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            source_function_registry_receiving::receive_case_with_temporal_arithmetic(
                case,
                &root["data_inputs"],
            )
        }));
        writeln!(
            std::io::stdout().lock(),
            "OE_TEMPORAL_DOMAIN_CASE {id} {}",
            if result.is_ok() { "ok" } else { "failed" }
        )
        .unwrap();
        if result.is_err() {
            failed.push(id);
        }
    }
    assert!(
        failed.is_empty(),
        "original temporal domain cases failed: {failed:?}"
    );
}
#[test]
fn source_temporal_domain_mixed_direct_false_observed() {
    receive_group(1, 192);
}
#[test]
fn source_temporal_domain_mixed_direct_true_observed() {
    receive_group(193, 192);
}
#[test]
fn source_temporal_domain_mixed_query_observed() {
    receive_group(385, 192);
}
#[test]
fn source_temporal_domain_initial_none_observed() {
    receive_group(577, 24);
}
#[test]
fn source_temporal_domain_initial_error_observed() {
    receive_group(601, 24);
}
#[test]
fn source_temporal_domain_language_or_iri_observed() {
    receive_group(625, 48);
}
#[test]
fn source_temporal_domain_callback_none_observed() {
    receive_group(673, 24);
}
#[test]
fn source_temporal_domain_callback_return_sparql_observed() {
    receive_group(697, 24);
}
#[test]
fn source_temporal_domain_nested_evaluation_direct_false_observed() {
    receive_group(721, 24);
}
#[test]
fn source_temporal_domain_nested_evaluation_direct_true_observed() {
    receive_group(745, 24);
}
#[test]
fn source_temporal_domain_nested_evaluation_query_observed() {
    receive_group(769, 24);
}

#[test]
fn source_temporal_domain_constructors_observed() {
    let bytes = crate::private_test_fixtures::text_ref!(
        "release-inputs-03/vendor/spareval/src/eval/fixtures/source_temporal_domain_constructors_01.json"
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes.as_bytes())),
        "21b77dfffbc4c9c06c6161b7fa4d4a7c1b60235d08710f3ea3044750d221bff4"
    );
    let root = source_temporal_operands_receiving::decode_fixture(bytes).unwrap();
    assert_eq!(root["candidate_results_used"], false);
    let cases = root["graph_queries"].as_array().unwrap();
    assert_eq!(cases.len(), 19);
    let mut failed = Vec::new();
    for (i, case) in cases.iter().enumerate() {
        let id = format!("TC{:03}", i + 1);
        assert_eq!(case["input"]["id"], id);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            source_function_registry_receiving::receive_case_with_temporal_arithmetic(
                case,
                &root["data_inputs"],
            )
        }));
        writeln!(
            std::io::stdout().lock(),
            "OE_TEMPORAL_DOMAIN_CASE {id} {}",
            if result.is_ok() { "ok" } else { "failed" }
        )
        .unwrap();
        if result.is_err() {
            failed.push(id);
        }
    }
    assert!(
        failed.is_empty(),
        "original constructor cases failed: {failed:?}"
    );
}
