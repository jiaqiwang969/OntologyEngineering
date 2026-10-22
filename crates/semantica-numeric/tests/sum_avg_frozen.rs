//! Independent source/math answers. Oracle and adapter share an author;
//! production execution and its receipt are separate.
use semantica_numeric::{
    Binary64Fault, NumericFault, NumericLimits, binary64_divide_integers, python_sum_binary64_pair,
};
use serde_json::{Value, json};

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap()
}

#[test]
fn sum_avg_frozen_source_math_cases() {
    let root: Value = serde_json::from_str(include_str!("fixtures/sum_avg_cases.json")).unwrap();
    assert_eq!(root["source_record_id"], "QUERY-SUM-AVG-FROZEN-03");
    assert_eq!(
        root["source_record_sha256"],
        "f86864839e50f1902e18ee89a9315377ead0577ab3065fdc6407d73f25246566"
    );
    let cases = root["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 16);
    for case in cases {
        let id = text(case, "id");
        let input = &case["input"];
        let expected = &case["expected"];
        let limit = |name: &str| {
            case["numeric_limits_override"]
                .get(name)
                .unwrap_or(&root["environment"]["numeric_limits"][name])
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
        assert_eq!(case["decimal_signals_added_exact"], json!([]));
        let result = match text(case, "operation") {
            "python_sum_binary64_pair" => {
                let from_bits =
                    |name| f64::from_bits(u64::from_str_radix(text(input, name), 16).unwrap());
                Ok(python_sum_binary64_pair(
                    from_bits("first_bits_hex"),
                    from_bits("second_bits_hex"),
                ))
            }
            "binary64_divide_integers" => binary64_divide_integers(
                &text(input, "numerator").parse().unwrap(),
                &text(input, "denominator").parse().unwrap(),
                &limits,
            ),
            other => panic!("{id}: unexpected primitive {other}"),
        };
        match text(expected, "kind") {
            "value" => {
                let actual = result.unwrap_or_else(|error| panic!("{id}: {error:?}"));
                if expected["classification"] == "NaN" {
                    assert!(actual.is_nan(), "{id}");
                    assert_eq!(expected["sign_and_payload"], "not asserted");
                } else {
                    assert_eq!(
                        format!("{:016x}", actual.to_bits()),
                        text(expected, "bits_hex"),
                        "{id}"
                    );
                }
            }
            "fault" => match (text(expected, "reason"), result.expect_err(id)) {
                ("IntegerOverflow", Binary64Fault::IntegerOverflow)
                | ("ZeroDivision", Binary64Fault::ZeroDivision) => {}
                (
                    "Core.ResourceLimit",
                    Binary64Fault::Core(NumericFault::ResourceLimit { operation }),
                ) => {
                    assert_eq!(operation, text(expected, "operation"), "{id}");
                    assert_eq!(case["classification"], "active_safety_not_original_parity");
                }
                (expected, actual) => panic!("{id}: expected {expected}, got {actual:?}"),
            },
            other => panic!("{id}: unexpected frozen outcome {other}"),
        }
    }
}
