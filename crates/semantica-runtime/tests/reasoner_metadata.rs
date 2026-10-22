use semantica_contracts::{SemanticReasonPort, SemanticReasonRequest};
use semantica_runtime::{reasoner, runtime::RustSemanticRuntime};
use serde_json::{Value, json};

fn request(input: Value) -> Result<SemanticReasonRequest, serde_json::Error> {
    let mut value = input;
    value["schema"] = json!("semantica.reason-request/1");
    serde_json::from_value(value)
}

#[test]
fn structured_rule_metadata_source_cases() {
    let fixture: Value = semantica_contracts::json::protocol_json(
        include_str!("fixtures/rule-metadata-source-derived.json"),
        1024 * 1024,
    )
    .unwrap();
    assert_eq!(fixture["expectation_origin"], "source_derived");
    assert_eq!(fixture["original_execution"], false);
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 40);
    let mut failures = Vec::new();
    for case in cases {
        let actual = request(case["input"].clone())
            .map_err(|error| error.to_string())
            .and_then(|input| {
                RustSemanticRuntime
                    .reason(&input)
                    .map(|result| result.data)
                    .map_err(|error| format!("{}:{}:{}", error.phase, error.code, error.message))
            });
        if case["expected"]["outcome"] == "success" {
            match actual {
                Ok(value) if value == case["expected"]["result"] => {}
                other => failures.push(format!("{}: {other:?}", case["id"])),
            }
        } else {
            match actual {
                Ok(value) => {
                    failures.push(format!("{}: expected rejection, got {value:?}", case["id"]))
                }
                Err(error)
                    if case["expected"]["original_exception_class"] == "SemanticRuntimeError" =>
                {
                    if !error.contains("runtime_reason_convergence") {
                        failures.push(format!("{}: wrong failure phase: {error}", case["id"]));
                    }
                }
                Err(error) if case["id"] != "max_iterations_bool_rejected" => {
                    if !error.contains("runtime_reason_input") {
                        failures.push(format!("{}: wrong failure phase: {error}", case["id"]));
                    }
                }
                Err(_) => {}
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn metadata_port_preserves_request_identity_and_rejects_invalid_shape() {
    let input = request(json!({
        "facts":"A(a)",
        "rules":[{"rule_id":"checked","text":"IF A(a) THEN B(a)","confidence":0.25,"priority":3}],
        "max_iterations":50
    }))
    .unwrap();
    let result = RustSemanticRuntime.reason(&input).unwrap();
    assert_eq!(result.schema, "semantica.reason-result/1");
    assert_eq!(result.data["inferences"][0]["confidence"], 0.25);
    let expected_hash = semantica_runtime::sha256(
        semantica_contracts::json::canonical_json(&serde_json::to_value(&input).unwrap()).unwrap(),
    );
    assert_eq!(result.request_sha256, expected_hash);
    let mut changed = input.clone();
    changed.rules[0]["confidence"] = json!(0.75);
    let changed_result = RustSemanticRuntime.reason(&changed).unwrap();
    assert_ne!(result.request_sha256, changed_result.request_sha256);
    assert_eq!(changed_result.data["inferences"][0]["confidence"], 0.75);
    changed.schema = "semantica.reason-request/0".into();
    assert!(RustSemanticRuntime.reason(&changed).is_err());
    let mut wire = serde_json::to_value(&input).unwrap();
    wire["unknown"] = json!(true);
    assert!(serde_json::from_value::<SemanticReasonRequest>(wire).is_err());
    let mut changed = input;
    changed.rules[0]["unknown"] = json!(true);
    assert!(RustSemanticRuntime.reason(&changed).is_err());
    changed.rules[0].as_object_mut().unwrap().remove("unknown");
    changed.facts = json!(" ".repeat(reasoner::MAX_INPUT_BYTES));
    let error = RustSemanticRuntime.reason(&changed).unwrap_err();
    assert_eq!(error.code, "input_budget_exceeded");
    assert_eq!(error.phase, "contract");
}

#[test]
fn priorities_preserve_arbitrary_integer_order_without_float_or_i64_coercion() {
    let wire = r#"{"schema":"semantica.reason-request/1","facts":"A(a)","rules":[
        {"rule_id":"low","text":"IF A(a) THEN L(a)","priority":90071992547409920000000000000000001},
        {"rule_id":"high","text":"IF A(a) THEN H(a)","priority":90071992547409920000000000000000002},
        {"rule_id":"negative","text":"IF A(a) THEN N(a)","priority":-90071992547409920000000000000000003}
    ],"max_iterations":1}"#;
    let raw = semantica_contracts::json::protocol_json(wire, 1024 * 1024).unwrap();
    let input: SemanticReasonRequest = serde_json::from_value(raw.clone()).unwrap();
    let result = RustSemanticRuntime.reason(&input).unwrap();
    assert_eq!(result.data["rules"][0]["rule_id"], "high");
    assert_eq!(result.data["rules"][1]["rule_id"], "low");
    assert_eq!(
        result.data["rules"][2]["priority"],
        raw["rules"][2]["priority"]
    );
    assert_eq!(
        result.data["inferences"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["rule_id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["high", "low", "negative"]
    );
}

#[test]
fn ordered_fact_inputs_and_rule_budget_boundaries_reject_without_partial_closure() {
    let result = reasoner::forward_chain_values(
        &json!([" # comment\nA(z) ", "A(a)", "A(z)"]),
        &json!(["IF A(?x) THEN B(?x)"]),
        1,
    )
    .unwrap();
    assert_eq!(result["input_facts"], json!(["A(a)", "A(z)"]));
    assert_eq!(result["all_facts"], json!(["A(a)", "A(z)", "B(a)", "B(z)"]));
    for facts in [
        json!(["A(a)\nA(b)"]),
        json!(["# comment"]),
        json!([true]),
        json!({"fact":"A(a)"}),
    ] {
        assert!(reasoner::forward_chain_values(&facts, &json!(["IF A(a) THEN B(a)"]), 50).is_err());
    }
    let too_many = json!(vec!["IF A(a) THEN B(a)"; reasoner::MAX_RULES + 1]);
    let error = reasoner::forward_chain_values(&json!("A(a)"), &too_many, 50)
        .unwrap_err()
        .to_string();
    assert_eq!(error, "runtime_reason_count_limit");
    let too_large = json!(" ".repeat(reasoner::MAX_INPUT_BYTES + 1));
    assert_eq!(
        reasoner::forward_chain_values(&too_large, &json!("IF A(a) THEN B(a)"), 50)
            .unwrap_err()
            .to_string(),
        "runtime_reason_input_limit"
    );
    for max in [0, 1001] {
        assert_eq!(
            reasoner::forward_chain_values(&json!("A(a)"), &json!("IF A(a) THEN B(a)"), max)
                .unwrap_err()
                .to_string(),
            "runtime_reason_iteration_uncovered"
        );
    }
}
