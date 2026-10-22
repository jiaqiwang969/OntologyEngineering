//! Independent source-derived Q40 replay tests. No candidate result builds an oracle.
use crate::semantic_outcome::execute;
use oe_contracts::CodeReplayCase;
use oe_wire::json::{canonical_json, strict_json};
use serde_json::{Value, json};

const ORACLE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../runtime/q40-replay-cases.json"
));
const ORACLE_SHA: &str = "fb556d112c0302389240f83b536c82aa7e5079a36be7c2cec2645a9d8a5b51ce";
fn cases() -> Vec<CodeReplayCase> {
    assert_eq!(oe_core::sha256(ORACLE), ORACLE_SHA);
    serde_json::from_value(strict_json(ORACLE).unwrap()).unwrap()
}

#[test]
fn source_q40_all16_successes_and24_typed_rejections_are_exact() {
    let cases = cases();
    assert_eq!(cases.len(), 40);
    let (mut successes, mut rejected, mut decode, mut reason_rejected) = (0, 0, 0, 0);
    for case in cases {
        assert_eq!(
            oe_core::sha256(canonical_json(&case.input).unwrap()),
            case.input_sha256
        );
        let result =
            execute(&case.input).unwrap_or_else(|error| panic!("{}: {error}", case.case_id));
        let mut actual = serde_json::to_value(result).unwrap();
        assert_eq!(
            actual["request_sha256"], case.input_sha256,
            "{}",
            case.case_id
        );
        match actual["outcome"].as_str().unwrap() {
            "success" => successes += 1,
            "rejected" => rejected += 1,
            other => panic!("unknown outcome {other}"),
        }
        if case.case_id == "max_iterations_bool_rejected" {
            decode += 1;
            assert_eq!(actual["stage"], "request_decode");
            assert_eq!(actual["reason_port_invoked"], false);
            assert_eq!(actual["failure"]["phase"], "request_decode");
            assert_eq!(actual["failure"]["code"], "invalid_reason_request");
            assert_eq!(actual["failure"]["retryable"], false);
            assert!(
                actual["failure"]["message"]
                    .as_str()
                    .is_some_and(|s| !s.trim().is_empty())
            );
            // Only serde spelling is intentionally outside the oracle contract.
            actual["failure"]["message"] = case.expected["failure"]["message"].clone();
        } else if actual["outcome"] == "rejected" {
            reason_rejected += 1;
        }
        assert_eq!(
            canonical_json(&actual).unwrap(),
            canonical_json(&case.expected).unwrap(),
            "{}",
            case.case_id
        );
    }
    assert_eq!(
        (successes, rejected, decode, reason_rejected),
        (16, 24, 1, 23)
    );
}

#[test]
fn adapter_preserves_adjacent_arbitrary_integer_priorities_and_full_result() {
    // Separately frozen control. It is NOT a forty-first member of Q40.
    let control = strict_json(include_bytes!("fixtures/arbitrary-priority-control.json")).unwrap();
    assert_eq!(control["historical_q40_member"], false);
    assert_eq!(control["original_execution"], false);
    let actual = execute(&control["request"]).unwrap();
    assert_eq!(actual.outcome, "success");
    assert_eq!(actual.request_sha256, control["request_sha256"]);
    assert_eq!(
        canonical_json(&serde_json::to_value(actual.result.unwrap()).unwrap()).unwrap(),
        canonical_json(&control["expected_semantic_result"]).unwrap()
    );
}

#[test]
fn unclassified_decode_contract_and_resource_errors_are_not_semantic_rejections() {
    let valid = cases()[0].input.clone();
    let mut unknown = valid.clone();
    unknown["caller_claimed_passed"] = json!(true);
    let mut wrong_schema = valid.clone();
    wrong_schema["schema"] = json!("semantica.reason-request/0");
    let mut missing = valid.clone();
    missing.as_object_mut().unwrap().remove("rules");
    let mut wrong_integer = valid.clone();
    wrong_integer["max_iterations"] = json!("true");
    let mut zero = valid.clone();
    zero["max_iterations"] = json!(0);
    let mut outside_profile = valid.clone();
    outside_profile["max_iterations"] = json!(1001);
    let mut large = valid.clone();
    large["facts"] = json!(" ".repeat(1024 * 1024));
    let mut count = valid.clone();
    count["rules"] = json!(vec!["IF A(a) THEN B(a)"; 257]);
    for value in [
        unknown,
        wrong_schema,
        missing,
        wrong_integer,
        zero,
        outside_profile,
        large,
        count,
        Value::Null,
    ] {
        assert!(
            execute(&value).is_err(),
            "unclassified failure became semantic rejection"
        );
    }
    let mut bool_and_unknown = valid;
    bool_and_unknown["max_iterations"] = json!(true);
    bool_and_unknown["caller_claimed_passed"] = json!(true);
    assert!(
        execute(&bool_and_unknown).is_err(),
        "boolean cannot mask unrelated DTO shape failure"
    );
}

#[test]
fn plain_mapping_marker_is_preserved_and_actual_condition_is_wire_guard() {
    let case = cases()
        .into_iter()
        .find(|c| c.case_id == "plain_mapping_iterable_element_is_not_original_dto")
        .unwrap();
    assert_eq!(case.input["rules"][0]["__original_python_type__"], "dict");
    let rejected = execute(&case.input).unwrap();
    assert_eq!(rejected.outcome, "rejected");
    assert_eq!(rejected.condition.as_deref(), Some("invalid_dto_fields"));
    assert_eq!(rejected.stage, "reason");
    assert!(rejected.reason_port_invoked);
    let mut explicit_dto = case.input;
    explicit_dto["rules"][0]
        .as_object_mut()
        .unwrap()
        .remove("__original_python_type__");
    let actual = execute(&explicit_dto).unwrap();
    assert_eq!(actual.outcome, "success");
    assert_ne!(actual.request_sha256, rejected.request_sha256);
    assert_eq!(
        actual.result.unwrap().data["all_facts"],
        json!(["A(a)", "B(a)"])
    );
}
