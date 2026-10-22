//! Source/math frozen answers, independently derived before candidate execution.
//! The oracle and adapter share an author. No candidate supplies expected bits.
//! These APIs have no Decimal context or signal channel; no ambient flags are
//! invented here. Persistent query flags are received in spareval's query tests.
use semantica_numeric::{
    Binary64BinaryOp, Binary64Fault, Binary64UnaryOp, DecimalValue, NumericFault, NumericLimits,
    apply_binary64_binary, apply_binary64_unary, binary64_from_decimal, binary64_from_integer,
};
use serde_json::{Value, json};

fn corpus() -> Value {
    let value: Value =
        serde_json::from_str(include_str!("fixtures/binary64_mixed_cases.json")).unwrap();
    assert_eq!(value["source_record_id"], "QUERY-BINARY64-FROZEN-01");
    assert_eq!(
        value["source_record_sha256"],
        "69974deeacb163228a361b5b3d03de93ee2fe437c1d5a95361b7d7de62ed34dd"
    );
    assert_eq!(value["core_cases"].as_array().unwrap().len(), 42);
    value
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap()
}

fn decimal(value: &Value) -> DecimalValue {
    match text(value, "kind") {
        "finite" => DecimalValue::Finite {
            negative: value["negative"].as_bool().unwrap(),
            coefficient: text(value, "coefficient").parse().unwrap(),
            exponent: text(value, "exponent").parse().unwrap(),
        },
        "infinity" => DecimalValue::Infinity {
            negative: value["negative"].as_bool().unwrap(),
        },
        "nan" => DecimalValue::NaN {
            negative: value["negative"].as_bool().unwrap(),
            signaling: value["signaling"].as_bool().unwrap(),
            payload: text(value, "payload").to_owned(),
        },
        kind => panic!("unexpected frozen Decimal kind {kind}"),
    }
}

fn binary64(value: &Value) -> f64 {
    assert_eq!(value["family"], "Binary64");
    let bits = text(value, "bits_hex");
    assert_eq!(bits.len(), 16);
    f64::from_bits(u64::from_str_radix(bits, 16).unwrap())
}

fn receive(root: &Value, case: &Value) {
    let id = text(case, "id");
    let input = &case["input"];
    let limit = |key: &str| {
        case["numeric_limits_override"]
            .get(key)
            .unwrap_or(&root["explicit_environment"]["numeric_limits"][key])
            .as_u64()
            .unwrap()
            .try_into()
            .unwrap()
    };
    let limits = NumericLimits {
        max_input_characters: limit("max_input_characters"),
        max_coefficient_digits: limit("max_coefficient_digits"),
        max_output_characters: limit("max_output_characters"),
        max_materialized_power_of_ten: limit("max_materialized_power_of_ten"),
    };
    // Inputs are already constructed values. Using the lexical API here would
    // wrongly reject BA05 before reaching the conversion API under test.
    let result = match text(case, "operation") {
        "binary64_from_integer" => {
            binary64_from_integer(&text(input, "value").parse().unwrap(), &limits)
        }
        "binary64_from_decimal" => binary64_from_decimal(&decimal(&input["value"]), &limits),
        "apply_binary64_binary" => {
            let op = match text(input, "op") {
                "Add" => Binary64BinaryOp::Add,
                "Subtract" => Binary64BinaryOp::Subtract,
                "Multiply" => Binary64BinaryOp::Multiply,
                "Divide" => Binary64BinaryOp::Divide,
                other => panic!("unknown binary operation {other}"),
            };
            apply_binary64_binary(op, binary64(&input["left"]), binary64(&input["right"]))
        }
        "apply_binary64_unary" => {
            let op = match text(input, "op") {
                "Plus" => Binary64UnaryOp::Plus,
                "Minus" => Binary64UnaryOp::Minus,
                other => panic!("unknown unary operation {other}"),
            };
            Ok(apply_binary64_unary(op, binary64(&input["value"])))
        }
        other => panic!("unknown frozen operation {other}"),
    };
    let expected = &case["expected"];
    assert_eq!(
        expected["decimal_signals_added_exact"],
        json!([]),
        "{id}: API has no Decimal signal channel"
    );
    match text(expected, "kind") {
        "value" => {
            let actual = result.unwrap_or_else(|error| panic!("{id}: unexpected {error:?}"));
            let value = &expected["value"];
            if value["classification"] == "NaN" {
                assert!(actual.is_nan(), "{id}: expected NaN class only");
                assert_eq!(value["sign_and_payload"], "not asserted");
            } else {
                assert_eq!(actual.to_bits(), binary64(value).to_bits(), "{id}");
            }
        }
        "fault" => {
            let actual = result.expect_err(id);
            match (text(expected, "reason"), actual) {
                ("IntegerOverflow", Binary64Fault::IntegerOverflow)
                | ("SignalingNaNConversion", Binary64Fault::SignalingNaNConversion)
                | ("ZeroDivision", Binary64Fault::ZeroDivision) => {}
                (
                    "Core.ResourceLimit",
                    Binary64Fault::Core(NumericFault::ResourceLimit { operation }),
                ) => {
                    assert_eq!(operation, text(case, "expected_operation"), "{id}");
                }
                (wanted, actual) => panic!("{id}: expected {wanted}, got {actual:?}"),
            }
        }
        other => panic!("unknown frozen outcome {other}"),
    }
}

#[test]
fn binary64_reverse_conversion_matches_frozen_source_math() {
    let root = corpus();
    let mut count = 0;
    for case in root["core_cases"].as_array().unwrap() {
        if text(case, "id").starts_with("BC")
            && text(case, "operation").starts_with("binary64_from_")
        {
            receive(&root, case);
            count += 1;
        }
    }
    assert_eq!(count, 24);
}

#[test]
fn binary64_scalar_operations_match_frozen_source_math() {
    let root = corpus();
    let mut count = 0;
    for case in root["core_cases"].as_array().unwrap() {
        if text(case, "operation").starts_with("apply_binary64_") {
            receive(&root, case);
            count += 1;
        }
    }
    assert_eq!(count, 13);
}

#[test]
fn binary64_conversion_resource_contract_matches_frozen_cases() {
    let root = corpus();
    let mut count = 0;
    for case in root["core_cases"].as_array().unwrap() {
        if text(case, "id").starts_with("BA") {
            receive(&root, case);
            count += 1;
        }
    }
    assert_eq!(count, 5);
}
