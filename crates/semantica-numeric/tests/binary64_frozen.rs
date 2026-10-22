//! Frozen reference-source/mathematical values, never candidate-derived answers.
//! Signals below are per-call deltas with initially clear flags. The core has
//! no persistent ambient Context; this adapter does not claim to test one.
use semantica_numeric::{
    DecimalContext, DecimalSignal, DecimalSignals, DecimalValue, NumericFault, NumericLimits,
    NumericOutcome, RoundingMode, decimal_from_binary64,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn corpus() -> Value {
    serde_json::from_str(include_str!("fixtures/source-derived-binary64.json")).unwrap()
}

fn signal(name: &str) -> DecimalSignal {
    DecimalSignal::ALL
        .into_iter()
        .find(|s| format!("{s:?}") == name)
        .unwrap_or_else(|| panic!("unknown frozen signal {name}"))
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
    assert_eq!(value["initial_flags"], json!([]));
    assert_eq!(value["ignored_flags"], json!([]));
    DecimalContext {
        precision: value["precision"].as_u64().unwrap().try_into().unwrap(),
        rounding: RoundingMode::HalfEven,
        emin: value["emin"].as_str().unwrap().parse().unwrap(),
        emax: value["emax"].as_str().unwrap().parse().unwrap(),
        clamp: match value["clamp"].as_u64().unwrap() {
            0 => false,
            1 => true,
            _ => panic!("unsupported frozen clamp"),
        },
        traps: signals(&value["traps"]),
    }
}

fn limits(value: &Value) -> NumericLimits {
    let get = |key| value[key].as_u64().unwrap().try_into().unwrap();
    NumericLimits {
        max_input_characters: get("max_input_characters"),
        max_coefficient_digits: get("max_coefficient_digits"),
        max_output_characters: get("max_output_characters"),
        max_materialized_power_of_ten: get("max_materialized_power_of_ten"),
    }
}

fn representation(value: &DecimalValue) -> Value {
    match value {
        DecimalValue::Finite {
            negative,
            coefficient,
            exponent,
        } => {
            json!({"kind":"finite", "negative":negative, "coefficient":coefficient.to_string(), "exponent":exponent.to_string()})
        }
        DecimalValue::Infinity { negative } => {
            json!({"kind":"infinity", "negative":negative})
        }
        DecimalValue::NaN {
            negative,
            signaling,
            payload,
        } => json!({"kind":"nan", "negative":negative, "signaling":signaling, "payload":payload}),
    }
}

fn run_case(case: &Value, corpus: &Value) {
    let bits = case["input"]["bits_hex"].as_str().unwrap();
    assert_eq!(bits.len(), 16);
    let input = f64::from_bits(u64::from_str_radix(bits, 16).unwrap());
    let ctx = context(&corpus["contexts"][case["ambient_context"].as_str().unwrap()]);
    let budget = limits(&corpus["interface_contract"]["limits"]);
    let result = match case["operation"].as_str().unwrap() {
        "binary64.from_float_exact" => {
            // The explicit API takes no context and exposes no signal channel.
            // Success must remain exact even though the documented ambient
            // context traps FloatOperation, Inexact, Rounded and range errors.
            assert!(ctx.traps.contains(DecimalSignal::FloatOperation));
            decimal_from_binary64(input, &budget).map(|value| NumericOutcome {
                value,
                signals: DecimalSignals::EMPTY,
            })
        }
        "binary64.decimal_constructor" => ctx.from_binary64(input, &budget),
        other => panic!("unknown frozen operation {other}"),
    };
    let expected = &case["expected"];
    // Initial flags are empty. This checks the source-derived one-call signal
    // projection, not mutation or accumulation in a global Python Context.
    assert_eq!(expected["ambient_flags_after"], expected["signals_exact"]);
    match expected["kind"].as_str().unwrap() {
        "value" => {
            let actual = result.unwrap();
            assert_eq!(representation(&actual.value), expected["value"]);
            assert_eq!(actual.signals, signals(&expected["signals_exact"]));
        }
        "trap" => {
            assert_eq!(expected["result_absent"], true);
            assert_eq!(expected["conversion_not_reached"], true);
            let Err(NumericFault::DecimalTrap {
                primary,
                signals: actual,
            }) = result
            else {
                panic!("expected a Decimal trap without a value; got {result:?}")
            };
            assert_eq!(primary, signal(expected["primary"].as_str().unwrap()));
            assert_eq!(actual, signals(&expected["signals_exact"]));
            // Independent active-budget controls below additionally distinguish
            // conversion-before-trap from the required trap-before-conversion.
        }
        other => panic!("unknown frozen expectation {other}"),
    }
}

#[test]
fn all_sixteen_frozen_binary64_cases_preserve_representation_and_signals() {
    let corpus = corpus();
    assert_eq!(
        corpus["source_record_sha256"],
        "f06930161fcbe451e0fcdfa6e118c46d44af6f5aeb059b10dcc97266efa92892"
    );
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(corpus["case_count"], 16);
    assert_eq!(cases.len(), 16);
    let mut ids = BTreeSet::new();
    let mut explicit = 0;
    let mut constructor = 0;
    for case in cases {
        let id = case["id"].as_str().unwrap();
        assert!(ids.insert(id));
        assert!(matches!(
            case["classification"].as_str().unwrap(),
            "source_derivation" | "source_and_mathematical_derivation"
        ));
        match case["operation"].as_str().unwrap() {
            "binary64.from_float_exact" => explicit += 1,
            "binary64.decimal_constructor" => constructor += 1,
            other => panic!("unknown operation {other}"),
        }
        let result = std::panic::catch_unwind(|| run_case(case, &corpus));
        assert!(result.is_ok(), "frozen case {id} failed");
    }
    assert_eq!((explicit, constructor), (14, 2));
}

#[test]
fn active_conversion_budget_keeps_float_trap_before_value_construction() {
    // Active resource-policy controls, deliberately outside the 16 source
    // cases: a legal precision-3 context with a too-small value budget.
    let corpus = corpus();
    let tiny = NumericLimits {
        max_coefficient_digits: 3,
        ..limits(&corpus["interface_contract"]["limits"])
    };
    let point_one = f64::from_bits(0x3fb9_9999_9999_999a);
    let resource = NumericFault::ResourceLimit {
        operation: "coefficient_digits",
    };
    assert_eq!(
        decimal_from_binary64(point_one, &tiny),
        Err(resource.clone())
    );
    let untrapped = context(&corpus["contexts"]["tiny_precision_float_untrapped"]);
    assert_eq!(untrapped.from_binary64(point_one, &tiny), Err(resource));
    let trapped = context(&corpus["contexts"]["tiny_precision_with_float_trap"]);
    assert_eq!(
        trapped.from_binary64(point_one, &tiny),
        Err(NumericFault::DecimalTrap {
            primary: DecimalSignal::FloatOperation,
            signals: DecimalSignal::FloatOperation.into(),
        })
    );

    // Conversion materializes a fixed, finite pow5 and never requests pow10
    // or output formatting. These two budgets must not silently become pow5
    // restrictions or a display/reparse path.
    let no_pow10_or_display = NumericLimits {
        max_materialized_power_of_ten: 0,
        max_output_characters: 0,
        ..limits(&corpus["interface_contract"]["limits"])
    };
    let expected = &corpus["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["id"] == "BF03_POINT_ONE_EXACT")
        .unwrap()["expected"]["value"];
    assert_eq!(
        representation(&decimal_from_binary64(point_one, &no_pow10_or_display).unwrap()),
        *expected
    );
}
