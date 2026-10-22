//! Characterisation of `semantica/utils/skos.py` beyond the original tests (expected values
//! from the oracle).

mod common;

use common::oracle;
use sem_utils::skos::{is_skos_hierarchy_edge, validate_skos_hierarchy};
use serde_json::Value;

#[test]
fn test_is_skos_hierarchy_edge_matches_oracle() {
    for case in oracle("is_skos_hierarchy_edge").as_array().unwrap() {
        assert_eq!(
            is_skos_hierarchy_edge(&case[0]),
            case[1].as_bool().unwrap(),
            "{}",
            case[0]
        );
    }
}

#[test]
fn test_validate_skos_hierarchy_matches_oracle() {
    for case in oracle("validate_skos_hierarchy").as_array().unwrap() {
        let new_edges: Vec<Value> = case["new"].as_array().unwrap().clone();
        let existing: Vec<Value> = case["existing"].as_array().unwrap().clone();
        let result = validate_skos_hierarchy(&new_edges, Some(&existing));
        match case.get("error") {
            None => assert!(result.is_ok(), "{case}: {result:?}"),
            Some(error) => {
                let err = result.expect_err("expected an error");
                assert_eq!(err.type_name(), error.as_str().unwrap(), "{case}");
                // Python visits a set, so which concept of the cycle is named varies between
                // runs; the message shape does not.
                let message = err.to_string();
                assert!(
                    message.starts_with("SKOS hierarchy contains a cycle involving '")
                        && message.ends_with("'."),
                    "{message}"
                );
                let expected = case["message"].as_str().unwrap();
                assert_eq!(&expected[..43], &message[..43]);
            }
        }
    }
}
