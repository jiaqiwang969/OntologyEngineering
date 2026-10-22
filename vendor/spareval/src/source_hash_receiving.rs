//! Frozen original built-in hash observations. This consumer contains no hash
//! implementation and never imports or launches the original interpreter.
use super::source_hash::*;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

const FIXTURE: &str = include_str!("eval/fixtures/source_context_set_cases.json");

fn fixture() -> Value {
    assert_eq!(
        format!("{:x}", Sha256::digest(FIXTURE.as_bytes())),
        "8c6ac659d5b1c79bccbb8ce646a6f2f108890252dc1ac5179b943213ca153977"
    );
    let root: Value = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(
        root["source_record_sha256"],
        "9069b3597b3867008827f74397d9bacdab7eb43dcf941f9d1d715d314d45eeb4"
    );
    root
}

fn word(value: &Value) -> i64 {
    value.as_str().unwrap().parse().unwrap()
}

fn receive(seed: u32) {
    let root = fixture();
    let case = root["hash_cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["seed"] == seed)
        .unwrap();
    let secret = secret_from_seed(seed);
    for item in case["strings"].as_array().unwrap() {
        assert_eq!(
            hash_python_string(item["text"].as_str().unwrap(), &secret),
            word(&item["hash"]),
            "seed={seed} text={:?}",
            item["text"]
        );
    }
    assert_eq!(hash_source_none(), word(&case["none_hash"]));
    for item in case["slot_normalization"].as_array().unwrap() {
        assert_eq!(
            normalize_hash_word(word(&item["input"])),
            word(&item["hash"])
        );
    }
    let mut literals = BTreeMap::new();
    for input in root["hash_inputs"]["literal_hash_cases"]
        .as_array()
        .unwrap()
    {
        let id = input["id"].as_str().unwrap();
        let expected = case["literal_source_recipes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["id"] == id)
            .unwrap();
        // The fixed language inputs are ASCII; source metadata owns lowering.
        let lower = input["language"].as_str().map(str::to_ascii_lowercase);
        let actual = hash_source_literal(
            input["lexical"].as_str().unwrap(),
            lower.as_deref(),
            input["datatype"].as_str(),
            &secret,
        );
        assert_eq!(actual, word(&expected["hash"]), "seed={seed} {id}");
        literals.insert(id, actual);
    }
    for input in root["hash_inputs"]["frozen_xor_cases"].as_array().unwrap() {
        let id = input["id"].as_str().unwrap();
        let expected = case["frozen_source_recipes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["id"] == id)
            .unwrap();
        let components: Vec<i64> = input["components"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| match item["kind"].as_str().unwrap() {
                "string" => hash_python_string(item["value"].as_str().unwrap(), &secret),
                "literal" => literals[item["id"].as_str().unwrap()],
                "none" => hash_source_none(),
                "raw_hash_word" => word(&item["value"]),
                kind => panic!("unknown fixed input component {kind}"),
            })
            .collect();
        let expected_components: Vec<i64> = expected["component_hashes"]
            .as_array()
            .unwrap()
            .iter()
            .map(word)
            .collect();
        assert_eq!(components, expected_components, "seed={seed} {id}");
        let cached_raw = xor_hash_words(components);
        assert_eq!(
            cached_raw,
            word(&expected["cached_xor"]),
            "seed={seed} {id}"
        );
        assert_eq!(
            normalize_hash_word(cached_raw),
            word(&expected["hash"]),
            "seed={seed} {id}"
        );
    }
}

#[test]
fn source_hash_original_seed_zero() {
    receive(0);
}
#[test]
fn source_hash_original_seed_one() {
    receive(1);
}
#[test]
fn source_hash_original_seed_u32_max() {
    receive(u32::MAX);
}
