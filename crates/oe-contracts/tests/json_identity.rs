use oe_contracts::{CodeComparison, FailureRecord};
use serde_json::{Value, json};
fn marker() -> Value {
    json!({"$serde_json::private::Number":"42"})
}
fn comparison() -> Value {
    json!({"schema":"oev2.code-comparison/1","assessment":"unknown","baseline_sha256":"a".repeat(64),"candidate_sha256":"b".repeat(64),"cost":"unknown","driver_sha256":"c".repeat(64),"engineering_adoption":"not_established","evaluator_sha256":"d".repeat(64),"experiment_id":"frozen-identity","fixed_denominator":6,"oracle_sha256":"e".repeat(64),"slots":[marker(),{"nested":[marker()]}],"uncovered":["identity probe; not activation evidence"],"version_selection":"not_performed"})
}
fn preserved(value: &Value) {
    assert!(
        value.is_object(),
        "shared engineering DTO must preserve literal object identity"
    );
    assert_eq!(value["$serde_json::private::Number"], "42");
}
#[test]
fn engineering_vec_value_preserves_from_str_and_from_value_identity() {
    let original = comparison();
    let raw = serde_json::to_string(&original).unwrap();
    for actual in [
        serde_json::from_str::<CodeComparison>(&raw).unwrap(),
        serde_json::from_value::<CodeComparison>(original).unwrap(),
    ] {
        preserved(&actual.slots[0]);
        preserved(&actual.slots[1]["nested"][0]);
    }
}
#[test]
fn engineering_optional_value_preserves_partial_result_without_promoting_it() {
    let original = json!({"code":"invalid_contract","phase":"frozen-test","retryable":false,"partial_result":marker()});
    let actual: FailureRecord = serde_json::from_value(original.clone()).unwrap();
    preserved(actual.partial_result.as_ref().unwrap());
    let actual: FailureRecord =
        serde_json::from_str(&serde_json::to_string(&original).unwrap()).unwrap();
    preserved(actual.partial_result.as_ref().unwrap());
    let absent: FailureRecord = serde_json::from_str(
        r#"{"code":"invalid_contract","phase":"frozen-test","retryable":false}"#,
    )
    .unwrap();
    assert!(absent.partial_result.is_none());
}
#[test]
fn engineering_vec_value_keeps_array_requirement_and_unknown_field_rejection() {
    let mut wrong = comparison();
    wrong["slots"] = marker();
    assert!(serde_json::from_value::<CodeComparison>(wrong).is_err());
    let mut wrong = comparison();
    wrong["eligible"] = json!(true);
    assert!(serde_json::from_value::<CodeComparison>(wrong).is_err());
}
#[test]
fn required_json_vec_missing_and_null_never_become_empty() {
    for null in [false, true] {
        let mut input = comparison();
        if null {
            input["slots"] = Value::Null;
        } else {
            input.as_object_mut().unwrap().remove("slots");
        }
        assert!(serde_json::from_value::<CodeComparison>(input).is_err());
    }
}
