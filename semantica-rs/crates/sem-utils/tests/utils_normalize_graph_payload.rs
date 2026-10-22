//! Characterisation of `normalize_graph_payload` and `_require_mapping`
//! (`semantica/utils/helpers.py`). The payloads are the scenarios of the original
//! `tests/utils/test_normalize_graph_payload.py` that JSON can express; expected results come
//! from the oracle. (That test file itself belongs to a later owner because it also drives the
//! exporters.)

mod common;

use common::{err_json, expected_outcome, oracle};
use sem_utils::helpers::NormalizedGraphPayload;
use sem_utils::normalize_graph_payload;
use serde::Serialize;
use serde_json::json;

#[test]
fn test_normalize_graph_payload_matches_oracle() {
    let cases = oracle("normalize_graph_payload").as_array().unwrap();
    assert!(cases.len() >= 40);
    for case in cases {
        let got = match normalize_graph_payload(&case["payload"]) {
            Ok(result) => json!({"ok": result.to_value()}),
            Err(e) => err_json(&e),
        };
        assert_eq!(got, expected_outcome(case), "payload {}", case["payload"]);
    }
}

#[test]
fn test_result_does_not_alias_the_input_collections() {
    let payload = json!({"entities": [{"id": "e1", "name": "Acme"}]});
    let mut result = normalize_graph_payload(&payload).unwrap();
    result
        .entities
        .push(json!({"id": "e2"}).as_object().unwrap().clone());
    assert_eq!(payload["entities"].as_array().unwrap().len(), 1);
}

#[test]
fn test_typed_records_are_accepted_once_serialized() {
    // Dataclass / attribute-bearing records are converted to dicts at the boundary in Python;
    // Rust callers serialize typed records to JSON objects.
    #[derive(Serialize)]
    struct Node {
        id: &'static str,
        name: &'static str,
    }
    let payload =
        json!({"entities": [serde_json::to_value(Node { id: "e1", name: "Acme" }).unwrap()]});
    let result = normalize_graph_payload(&payload).unwrap();
    assert_eq!(
        result,
        NormalizedGraphPayload {
            entities: vec![
                json!({"id": "e1", "name": "Acme"})
                    .as_object()
                    .unwrap()
                    .clone()
            ],
            relationships: vec![],
            triplets: vec![],
        }
    );
    assert_eq!(
        serde_json::to_value(&result).unwrap(),
        json!({"entities": [{"id": "e1", "name": "Acme"}], "relationships": [], "triplets": []})
    );
}

#[test]
fn test_error_names_supplied_and_expected_keys() {
    let err = normalize_graph_payload(&json!({"data": [{"id": "e1"}]})).unwrap_err();
    let message = err.to_string();
    for needle in ["data", "entities", "nodes"] {
        assert!(message.contains(needle), "{message}");
    }
    assert!(err.is_validation_error());
}
