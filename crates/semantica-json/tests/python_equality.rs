use semantica_json::{protocol_json, python_equal};
use serde_json::Value;

#[test]
fn independent_python_json_equality_38_original_cases() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/python_equality.json")).unwrap();
    assert_eq!(fixture["case_count"], 38);
    assert_eq!(
        fixture["input_sha256"],
        "f6d15831b2ac7a07c292167214f68b2bfbfa370bead28a8e13bb5318f6e5565e"
    );
    assert_eq!(
        fixture["oracle_sha256"],
        "d7196c820397d94f6633eaab5e753d07e2912abd0a8b2ca72a98e6948fe3c5e9"
    );
    for case in fixture["cases"].as_array().unwrap() {
        let left = protocol_json(case["left_json"].as_str().unwrap(), 1024 * 1024).unwrap();
        let right = protocol_json(case["right_json"].as_str().unwrap(), 1024 * 1024).unwrap();
        assert_eq!(
            python_equal(&left, &right).unwrap(),
            case["expected_equal"].as_bool().unwrap(),
            "independent original equality: {}",
            case["case_id"]
        );
        assert_eq!(
            python_equal(&right, &left).unwrap(),
            case["expected_equal"].as_bool().unwrap()
        );
    }
}
