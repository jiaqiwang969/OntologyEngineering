//! Execute independently frozen source/mathematical expectations. No Python,
//! candidate-produced oracle, deferred rounding, or lexical reparse of steps.
use semantica_numeric::{
    BigInt, DecimalContext, DecimalOp, DecimalSignal, DecimalSignals, DecimalValue, IntegerOp,
    NumericFault, NumericLimits, NumericOutcome, RoundingMode, apply_integer, decimal_from_integer,
    format_decimal_fixed, format_integer, parse_decimal_exact, parse_integer,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
enum Held {
    Integer(BigInt),
    Decimal(DecimalValue),
    Text(String),
}

fn signal(name: &str) -> DecimalSignal {
    DecimalSignal::ALL
        .into_iter()
        .find(|s| format!("{s:?}") == name)
        .unwrap()
}

fn signals(value: &Value) -> DecimalSignals {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|s| signal(s.as_str().unwrap()))
        .collect()
}

fn context(value: &Value) -> DecimalContext {
    assert_eq!(value["rounding"], "HALF_EVEN");
    DecimalContext {
        precision: value["precision"].as_u64().unwrap().try_into().unwrap(),
        rounding: RoundingMode::HalfEven,
        emin: value["emin"].as_str().unwrap().parse().unwrap(),
        emax: value["emax"].as_str().unwrap().parse().unwrap(),
        clamp: match value["clamp"].as_u64().unwrap() {
            0 => false,
            1 => true,
            _ => panic!("unknown clamp"),
        },
        traps: signals(&value["traps"]),
    }
}

fn limits(value: &Value, override_value: Option<&Value>) -> NumericLimits {
    let get = |key| {
        override_value
            .and_then(|v| v.get(key))
            .unwrap_or(&value[key])
            .as_u64()
            .unwrap()
            .try_into()
            .unwrap()
    };
    NumericLimits {
        max_input_characters: get("max_input_characters"),
        max_coefficient_digits: get("max_coefficient_digits"),
        max_output_characters: get("max_output_characters"),
        max_materialized_power_of_ten: get("max_materialized_power_of_ten"),
    }
}

fn integer(input: &Value, saved: &BTreeMap<String, Held>, limits: &NumericLimits) -> BigInt {
    if let Some(name) = input.get("ref") {
        let Held::Integer(value) = &saved[name.as_str().unwrap()] else {
            panic!("integer reference required")
        };
        value.clone()
    } else {
        parse_integer(input["integer"].as_str().unwrap(), limits, Some(4096)).unwrap()
    }
}

fn decimal(input: &Value, saved: &BTreeMap<String, Held>, limits: &NumericLimits) -> DecimalValue {
    if let Some(name) = input.get("ref") {
        match &saved[name.as_str().unwrap()] {
            Held::Decimal(value) => value.clone(),
            Held::Integer(value) => decimal_from_integer(value, limits).unwrap(),
            Held::Text(_) => panic!("text must never replace a cached numeric value"),
        }
    } else if let Some(value) = input.get("integer") {
        decimal_from_integer(
            &parse_integer(value.as_str().unwrap(), limits, Some(4096)).unwrap(),
            limits,
        )
        .unwrap()
    } else {
        parse_decimal_exact(input["decimal"].as_str().unwrap(), limits).unwrap()
    }
}

fn evaluate(
    step: &Value,
    contexts: &Value,
    saved: &BTreeMap<String, Held>,
    limits: &NumericLimits,
) -> Result<NumericOutcome<Held>, NumericFault> {
    let op = step["operation"].as_str().unwrap();
    let inputs = step["operands"].as_array().unwrap();
    if op.starts_with("integer.") {
        let value = if op == "integer.parse" {
            parse_integer(inputs[0]["text"].as_str().unwrap(), limits, Some(4096))?
        } else {
            let a = integer(&inputs[0], saved, limits);
            let b = inputs.get(1).map(|b| integer(b, saved, limits));
            let op = match op {
                "integer.add" => IntegerOp::Add,
                "integer.subtract" => IntegerOp::Subtract,
                "integer.multiply" => IntegerOp::Multiply,
                "integer.plus" => IntegerOp::Plus,
                "integer.minus" => IntegerOp::Minus,
                _ => panic!("unknown frozen integer operation: {op}"),
            };
            apply_integer(op, &a, b.as_ref(), limits)?
        };
        return Ok(NumericOutcome {
            value: Held::Integer(value),
            signals: DecimalSignals::EMPTY,
        });
    }
    let ctx = context(&contexts[step["context"].as_str().unwrap()]);
    if op == "decimal.parse_exact" {
        // The context constructor applies syntax traps but does not round valid input.
        let result = ctx.parse(inputs[0]["text"].as_str().unwrap(), limits)?;
        return Ok(NumericOutcome {
            value: Held::Decimal(result.value),
            signals: result.signals,
        });
    }
    let a = decimal(&inputs[0], saved, limits);
    if op == "decimal.format_fixed" {
        return Ok(NumericOutcome {
            value: Held::Text(format_decimal_fixed(&a, limits)?),
            signals: DecimalSignals::EMPTY,
        });
    }
    let b = inputs.get(1).map(|b| decimal(b, saved, limits));
    let op = match op {
        "decimal.add" => DecimalOp::Add,
        "decimal.subtract" => DecimalOp::Subtract,
        "decimal.multiply" => DecimalOp::Multiply,
        "decimal.divide" => DecimalOp::Divide,
        "decimal.plus" => DecimalOp::Plus,
        "decimal.minus" => DecimalOp::Minus,
        _ => panic!("unknown frozen decimal operation: {op}"),
    };
    let result = ctx.apply(op, &a, b.as_ref(), limits)?;
    Ok(NumericOutcome {
        value: Held::Decimal(result.value),
        signals: result.signals,
    })
}

fn representation(value: &Held, limits: &NumericLimits) -> Value {
    match value {
        Held::Integer(value) => {
            json!({"kind":"integer", "decimal":format_integer(value, limits, Some(4096)).unwrap()})
        }
        Held::Text(value) => json!({"kind":"text", "text":value}),
        Held::Decimal(DecimalValue::Finite {
            negative,
            coefficient,
            exponent,
        }) => {
            json!({"kind":"finite", "negative":negative, "coefficient":coefficient.to_string(), "exponent":exponent.to_string()})
        }
        Held::Decimal(DecimalValue::Infinity { negative }) => {
            json!({"kind":"infinity", "negative":negative})
        }
        Held::Decimal(DecimalValue::NaN {
            negative,
            signaling,
            payload,
        }) => json!({"kind":"nan", "negative":negative, "signaling":signaling, "payload":payload}),
    }
}

fn run_case(case: &Value, corpus: &Value) {
    let mut saved = BTreeMap::new();
    for step in case["steps"].as_array().unwrap() {
        let budget = limits(
            &corpus["interface_contract"]["safety_limits"],
            step.get("limits_override"),
        );
        let result = evaluate(step, &corpus["contexts"], &saved, &budget);
        let expected = &step["expected"];
        match expected["kind"].as_str().unwrap() {
            "value" => {
                let result = result.unwrap();
                assert_eq!(representation(&result.value, &budget), expected["value"]);
                assert_eq!(result.signals, signals(&expected["signals_exact"]));
                if let Some(fixed) = expected.get("fixed") {
                    let Held::Decimal(value) = &result.value else {
                        panic!("fixed Decimal expected")
                    };
                    assert_eq!(
                        format_decimal_fixed(value, &budget).unwrap(),
                        fixed.as_str().unwrap()
                    );
                }
                if let Some(name) = step.get("save") {
                    assert!(
                        saved
                            .insert(name.as_str().unwrap().to_owned(), result.value)
                            .is_none()
                    );
                }
            }
            "trap" => {
                assert_eq!(expected["result_absent"], true);
                let Err(NumericFault::DecimalTrap {
                    primary,
                    signals: actual,
                }) = result
                else {
                    panic!("expected Decimal trap; got {result:?}")
                };
                assert_eq!(primary, signal(expected["primary"].as_str().unwrap()));
                for required in signals(&expected["signals_required"]).iter() {
                    assert!(actual.contains(required));
                }
                if !expected["signals_exact"].is_null() {
                    assert_eq!(actual, signals(&expected["signals_exact"]));
                }
            }
            "resource_fault" => {
                assert_eq!(expected["result_absent"], true);
                assert_eq!(expected["numeric_trap"], false);
                let Err(NumericFault::ResourceLimit { operation }) = result else {
                    panic!("expected fatal resource limit; got {result:?}")
                };
                // Explicit API spelling adapter; it cannot accept another fault family.
                let category = match operation {
                    "numeric output characters" => "output_characters",
                    _ => panic!("unexpected resource operation: {operation}"),
                };
                assert_eq!(category, expected["category"].as_str().unwrap());
            }
            other => panic!("unknown frozen expectation: {other}"),
        }
    }
}

#[test]
fn all_seventy_frozen_cases_check_intermediate_values_signals_and_faults() {
    let corpus: Value =
        serde_json::from_str(include_str!("fixtures/source-derived-arithmetic.json")).unwrap();
    assert_eq!(
        corpus["source_record_sha256"],
        "17537b2c280da7ba85fe2939a05081653661564e340bb65c7ce5bd73f257a21a"
    );
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(corpus["case_count"], 70);
    assert_eq!(cases.len(), 70);
    let mut ids = BTreeSet::new();
    let mut failures = Vec::new();
    for case in cases {
        let id = case["id"].as_str().unwrap();
        assert!(ids.insert(id));
        if std::panic::catch_unwind(|| run_case(case, &corpus)).is_err() {
            failures.push(id);
        }
    }
    assert!(failures.is_empty(), "Frozen cases failed: {failures:?}");
}
