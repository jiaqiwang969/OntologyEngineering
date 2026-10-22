//! Frozen temporal operand observations, including separately recovered errors.
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
        crate::private_test_fixtures::text_ref!("release-inputs-03/vendor/spareval/src/eval/fixtures/source_temporal_operands_query_cases_01.json"),
        "9ced4111b2e7d37c81d8dfbfcbb3ca19d127a77adc5fade756312b2d312e0a9e",
    ),
    (
        crate::private_test_fixtures::text_ref!("release-inputs-03/vendor/spareval/src/eval/fixtures/source_temporal_operands_query_cases_02.json"),
        "5d94ac49e8f609917252411e0c79e111e7a2d4c07ceec5c2d15cd99ff567af9d",
    ),
    (
        crate::private_test_fixtures::text_ref!("release-inputs-03/vendor/spareval/src/eval/fixtures/source_temporal_operands_query_cases_03.json"),
        "860b5c6f2abc43f874087b92b10f1029ce64b08171624be38e83a1573dc00852",
    ),
    (
        crate::private_test_fixtures::text_ref!("release-inputs-03/vendor/spareval/src/eval/fixtures/source_temporal_operands_query_cases_04.json"),
        "4503654099ce13737d683e376ce6cf51d08c39960de7b520c39e4439eee36f9b",
    ),
    (
        crate::private_test_fixtures::text_ref!("release-inputs-03/vendor/spareval/src/eval/fixtures/source_temporal_operands_query_cases_05.json"),
        "d98a49897358426004bb080f92cff0388fbef7c71c84676b8a593c6da648b8c3",
    ),
    (
        crate::private_test_fixtures::text_ref!("release-inputs-03/vendor/spareval/src/eval/fixtures/source_temporal_operands_query_cases_06.json"),
        "4430fbed203c2a36f4b02764398d49eab29912bbbd5c781756bddc7fdf55bd67",
    ),
    (
        crate::private_test_fixtures::text_ref!("release-inputs-03/vendor/spareval/src/eval/fixtures/source_temporal_operands_query_cases_07.json"),
        "d041d4d3b671a0f188f53b129eb1fd117bfb801b93c3d4fe839356678945cdcf",
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
                let root: Value = decode_fixture(bytes).unwrap();
                assert_eq!(root["candidate_results_used"], false);
                root
            })
            .collect()
    })
}
fn recovered() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| {
        let bytes = crate::private_test_fixtures::text_ref!("release-inputs-03/vendor/spareval/src/eval/fixtures/source_temporal_operands_recovered_01.json");
        assert_eq!(
            format!("{:x}", Sha256::digest(bytes.as_bytes())),
            "83584ef89a735e82ec776b6841fafa8b2b2a63cc885265dc60b26a0cf20812a9"
        );
        let root: Value = decode_fixture(bytes).unwrap();
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
        762
    );
    let mut failed = Vec::new();
    for ordinal in first..first + count {
        let id = format!("TO{ordinal:03}");
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
        let (root, original) = members[0];
        let case = if original["status"] == "observer_error" {
            let cases: Vec<_> = recovered()["graph_queries"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|c| c["input"]["id"] == id)
                .collect();
            assert_eq!(cases.len(), 1);
            assert_eq!(cases[0]["input"], original["input"]);
            assert_eq!(recovered()["data_inputs"], root["data_inputs"]);
            cases[0]
        } else {
            original
        };
        assert!(case["status"] == "observed_complete" || case["status"] == "original_error");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            source_function_registry_receiving::receive_case_with_temporal_arithmetic(
                case,
                &root["data_inputs"],
            )
        }));
        writeln!(
            std::io::stdout().lock(),
            "OE_TEMPORAL_OPERANDS_CASE {id} {}",
            if result.is_ok() { "ok" } else { "failed" }
        )
        .unwrap();
        if result.is_err() {
            failed.push(id);
        }
    }
    assert!(
        failed.is_empty(),
        "original temporal operand cases failed: {failed:?}"
    );
}
#[test]
fn source_temporal_operands_invalid_time_direct_false_observed() {
    receive_group(1, 168);
}
#[test]
fn source_temporal_operands_invalid_time_direct_true_observed() {
    receive_group(169, 168);
}
#[test]
fn source_temporal_operands_invalid_time_query_observed() {
    receive_group(337, 168);
}
#[test]
fn source_temporal_operands_invalid_date_direct_false_observed() {
    receive_group(505, 24);
}
#[test]
fn source_temporal_operands_invalid_datetime_direct_false_observed() {
    receive_group(529, 24);
}
#[test]
fn source_temporal_operands_invalid_date_direct_true_observed() {
    receive_group(553, 24);
}
#[test]
fn source_temporal_operands_invalid_datetime_direct_true_observed() {
    receive_group(577, 24);
}
#[test]
fn source_temporal_operands_invalid_date_query_observed() {
    receive_group(601, 24);
}
#[test]
fn source_temporal_operands_invalid_datetime_query_observed() {
    receive_group(625, 24);
}
#[test]
fn source_temporal_operands_nested_direct_false_observed() {
    receive_group(649, 32);
}
#[test]
fn source_temporal_operands_nested_direct_true_observed() {
    receive_group(681, 32);
}
#[test]
fn source_temporal_operands_nested_query_observed() {
    receive_group(713, 32);
}
#[test]
fn source_temporal_operands_operand_domain_direct_false_observed() {
    receive_group(745, 6);
}
#[test]
fn source_temporal_operands_operand_domain_direct_true_observed() {
    receive_group(751, 6);
}
#[test]
fn source_temporal_operands_operand_domain_query_observed() {
    receive_group(757, 6);
}
#[test]
fn source_temporal_operands_observation_recovery() {
    let originals: Vec<_> = fixtures()
        .iter()
        .flat_map(|r| r["graph_queries"].as_array().unwrap())
        .filter(|c| c["status"] == "observer_error")
        .collect();
    assert_eq!(originals.len(), 102);
    let cases = recovered()["graph_queries"].as_array().unwrap();
    assert_eq!(cases.len(), 102);
    for c in originals {
        let matches: Vec<_> = cases.iter().filter(|v| v["input"] == c["input"]).collect();
        assert_eq!(matches.len(), 1);
        let r = matches[0];
        assert_eq!(r["status"], "original_error");
        assert_eq!(r["error_stage"], "first_poll");
        assert_eq!(r["original_boundary"], "next(bindings)");
        assert_eq!(r["trace"][0]["event"], "call");
        assert_eq!(r["trace"][1]["event"], "exception");
        assert_eq!(r["trace"][1]["exception"], r["error"]);
        assert_eq!(r["trace"][2]["event"], "return");
        assert!(r["trace"][2]["result"].is_null());
    }
}

// A test-source transport, not another evaluator or runtime execution path.
// Original JSON values remain frozen; raw archived bytes bind the earlier run.
pub(super) fn decode_fixture(bytes: &str) -> Result<Value, String> {
    use base64::Engine;
    use std::io::Read;
    if bytes.len() > 1024 * 1024 {
        return Err("packed_fixture_budget".into());
    }
    let envelope: Value = serde_json::from_str(bytes).map_err(|_| "packed_fixture_json")?;
    let object = envelope.as_object().ok_or("packed_fixture_object")?;
    let keys = [
        "schema",
        "codec",
        "decoded_bytes",
        "decoded_sha256",
        "original_fixture_sha256",
        "payload",
    ];
    if object.len() != keys.len()
        || !keys.iter().all(|k| object.contains_key(*k))
        || envelope["schema"] != "private.source-query-fixture/1"
        || envelope["codec"] != "gzip+base64"
    {
        return Err("packed_fixture_contract".into());
    }
    let size = envelope["decoded_bytes"]
        .as_u64()
        .ok_or("packed_fixture_size")?;
    if size == 0 || size > 4 * 1024 * 1024 {
        return Err("packed_fixture_decoded_budget".into());
    }
    for key in ["decoded_sha256", "original_fixture_sha256"] {
        let sha = envelope[key].as_str().ok_or("packed_fixture_digest")?;
        if sha.len() != 64
            || !sha
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("packed_fixture_digest".into());
        }
    }
    let compressed = base64::engine::general_purpose::STANDARD
        .decode(
            envelope["payload"]
                .as_str()
                .ok_or("packed_fixture_payload")?,
        )
        .map_err(|_| "packed_fixture_base64")?;
    let mut decoder = flate2::bufread::GzDecoder::new(compressed.as_slice());
    let mut decoded = Vec::new();
    (&mut decoder)
        .take(size + 1)
        .read_to_end(&mut decoded)
        .map_err(|_| "packed_fixture_gzip")?;
    if decoded.len() as u64 != size || !decoder.get_ref().is_empty() {
        return Err("packed_fixture_length_or_trailing".into());
    }
    if format!("{:x}", Sha256::digest(&decoded)) != envelope["decoded_sha256"].as_str().unwrap() {
        return Err("packed_fixture_decoded_digest".into());
    }
    serde_json::from_slice(&decoded).map_err(|_| "packed_fixture_decoded_json".into())
}
#[test]
fn source_temporal_operands_packed_complete() {
    assert_eq!(
        fixtures()
            .iter()
            .map(|r| r["graph_queries"].as_array().unwrap().len())
            .sum::<usize>(),
        762
    );
    assert_eq!(recovered()["graph_queries"].as_array().unwrap().len(), 102);
    for (bytes, _) in private_fixtures() {
        let encoded: Value = serde_json::from_str(bytes).unwrap();
        assert_eq!(encoded["schema"], "private.source-query-fixture/1");
        assert_eq!(
            decode_fixture(bytes).unwrap()["candidate_results_used"],
            false
        );
    }
}
#[test]
fn source_temporal_operands_packed_rejects_drift() {
    use base64::Engine;
    let original: Value = serde_json::from_str(private_fixtures()[0].0).unwrap();
    let mut bad = original.clone();
    bad["extra"] = Value::Bool(true);
    assert!(decode_fixture(&bad.to_string()).is_err());
    let mut bad = original.clone();
    bad["decoded_bytes"] = serde_json::json!(4 * 1024 * 1024 + 1);
    assert!(decode_fixture(&bad.to_string()).is_err());
    let mut bad = original.clone();
    bad["decoded_bytes"] = serde_json::json!(original["decoded_bytes"].as_u64().unwrap() - 1);
    assert!(decode_fixture(&bad.to_string()).is_err());
    let mut bad = original.clone();
    bad["decoded_sha256"] = Value::String("0".repeat(64));
    assert!(decode_fixture(&bad.to_string()).is_err());
    let mut bad = original.clone();
    let mut payload = base64::engine::general_purpose::STANDARD
        .decode(bad["payload"].as_str().unwrap())
        .unwrap();
    payload.push(0);
    bad["payload"] = Value::String(base64::engine::general_purpose::STANDARD.encode(payload));
    assert!(decode_fixture(&bad.to_string()).is_err());
    assert!(decode_fixture("{}").is_err());
    assert!(decode_fixture(private_fixtures()[0].0).is_ok()); // Rejection cannot poison the next decode.
}
