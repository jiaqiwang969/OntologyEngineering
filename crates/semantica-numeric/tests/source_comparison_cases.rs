//! Pre-implementation source/math answers, with separately identified resource
//! policy. Oracle and receiving adapter share an author; comparison production
//! and actual Cargo execution have different owners. No candidate yields an
//! expected value. Input structs are built directly, without a lexical oracle.
use semantica_numeric::{
    DecimalContext, DecimalSignal, DecimalSignals, DecimalValue, NumericFault, NumericLimits,
    NumericRelation, NumericValue,
};
use serde_json::{Value, json};

fn corpus() -> Value {
    let root: Value =
        serde_json::from_str(include_str!("fixtures/source_comparison_cases.json")).unwrap();
    assert_eq!(
        root["source_record_sha256"],
        "6df0965cedb36486e13c20876eab75315fb77e42c5af66a0b4278e3dbe5e55fa"
    );
    assert_eq!(root["source_math_cases"].as_array().unwrap().len(), 8);
    assert_eq!(root["active_contract_cases"].as_array().unwrap().len(), 1);
    root
}
fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap()
}
fn signal(name: &str) -> DecimalSignal {
    match name {
        "InvalidOperation" => DecimalSignal::InvalidOperation,
        "DivisionByZero" => DecimalSignal::DivisionByZero,
        "Overflow" => DecimalSignal::Overflow,
        "Underflow" => DecimalSignal::Underflow,
        "Subnormal" => DecimalSignal::Subnormal,
        "Inexact" => DecimalSignal::Inexact,
        "Rounded" => DecimalSignal::Rounded,
        "Clamped" => DecimalSignal::Clamped,
        "FloatOperation" => DecimalSignal::FloatOperation,
        other => panic!("unreceived signal {other}"),
    }
}
fn signals(value: &Value) -> DecimalSignals {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|name| signal(name.as_str().unwrap()))
        .collect()
}
fn number(value: &Value) -> NumericValue {
    match text(value, "family") {
        "Integer" => NumericValue::Integer(text(value, "value").parse().unwrap()),
        "Binary64" => NumericValue::Binary64(f64::from_bits(
            u64::from_str_radix(text(value, "bits").trim_start_matches("0x"), 16).unwrap(),
        )),
        "Decimal" => NumericValue::Decimal(match value["special"].as_str() {
            None => DecimalValue::Finite {
                negative: value["negative"].as_bool().unwrap(),
                coefficient: text(value, "coefficient").parse().unwrap(),
                exponent: text(value, "exponent").parse().unwrap(),
            },
            Some("NaN") => DecimalValue::NaN {
                negative: value["negative"].as_bool().unwrap(),
                signaling: value["signaling"].as_bool().unwrap(),
                payload: text(value, "payload").into(),
            },
            Some(other) => panic!("unreceived special {other}"),
        }),
        other => panic!("unreceived numeric family {other}"),
    }
}
fn context(root: &Value, input: &Value, run: &Value) -> DecimalContext {
    let base = &root["context"]["decimal_context"];
    assert_eq!(base["rounding"], "HALF_EVEN");
    let field = |key: &str| {
        run.get(key)
            .or_else(|| input["context_override"].get(key))
            .unwrap_or(&base[key])
            .clone()
    };
    DecimalContext {
        precision: field("precision").as_u64().unwrap() as usize,
        emin: field("Emin").as_i64().unwrap().into(),
        emax: field("Emax").as_i64().unwrap().into(),
        clamp: field("clamp").as_u64().unwrap() != 0,
        traps: signals(&field("traps")),
        ..DecimalContext::default()
    }
}
fn limits(root: &Value, input: &Value) -> NumericLimits {
    let field = |key: &str| {
        input["limits_override"]
            .get(key)
            .unwrap_or(&root["context"]["numeric_limits"][key])
            .as_u64()
            .unwrap() as usize
    };
    NumericLimits {
        max_input_characters: field("max_input_characters"),
        max_coefficient_digits: field("max_coefficient_digits"),
        max_output_characters: field("max_output_characters"),
        max_materialized_power_of_ten: field("max_materialized_power_of_ten"),
    }
}
fn receive(id: &str) {
    let root = corpus();
    let case = root["source_math_cases"]
        .as_array()
        .unwrap()
        .iter()
        .chain(root["active_contract_cases"].as_array().unwrap())
        .find(|case| case["id"] == id)
        .unwrap();
    let input = &case["input"];
    let expected = &case["expected"];
    let default_runs = vec![Value::Null];
    let runs = input["runs"].as_array().unwrap_or(&default_runs);
    let default_pairs = vec![json!([input["left"], input["right"]])];
    let pairs = input["pairs"].as_array().unwrap_or(&default_pairs);
    let default_ops = vec![input["operation"].clone()];
    let operations = input["operations"].as_array().unwrap_or(&default_ops);
    for (run_index, run) in runs.iter().enumerate() {
        let wanted_run = expected["runs"]
            .as_array()
            .map_or(expected, |runs| &runs[run_index]);
        for (pair_index, pair) in pairs.iter().enumerate() {
            let left = number(&pair[0]);
            let right = number(&pair[1]);
            for (op_index, op) in operations.iter().enumerate() {
                let operation = match op.as_str().unwrap() {
                    "Equal" => NumericRelation::Equal,
                    "Less" => NumericRelation::Less,
                    "Greater" => NumericRelation::Greater,
                    other => panic!("unreceived operation {other}"),
                };
                // A fresh explicit context for every frozen operation. It does
                // not execute Decimal parsing or arithmetic to derive answers.
                let actual = context(&root, input, run).compare_numeric(
                    operation,
                    &left,
                    &right,
                    &limits(&root, input),
                );
                let wanted = wanted_run["per_operation"]
                    .as_array()
                    .map_or(wanted_run, |values| &values[op_index]);
                assert_eq!(actual.signals, signals(&wanted["flags_exact"]), "{id}");
                let wanted_bool = wanted["value"]
                    .as_bool()
                    .or_else(|| wanted["results"][op_index].as_bool())
                    .or_else(|| wanted["results_per_pair"][pair_index][op_index].as_bool());
                if let Some(wanted) = wanted_bool {
                    assert_eq!(actual.result, Ok(wanted), "{id}");
                } else if let Some(class) = wanted["error_class"].as_str() {
                    let primary = signal(class.strip_prefix("decimal.").unwrap());
                    assert_eq!(
                        actual.result,
                        Err(NumericFault::DecimalTrap {
                            primary,
                            signals: signals(&wanted["flags_exact"]),
                        }),
                        "{id}"
                    );
                } else {
                    assert_eq!(wanted["fatal_kind"], "NumericFault::ResourceLimit");
                    let Err(NumericFault::ResourceLimit { operation }) = actual.result else {
                        panic!("{id}: expected conversion resource failure")
                    };
                    assert_eq!(operation, text(wanted, "operation"));
                }
            }
        }
    }
}

#[test]
fn source_comparison_fixture_identity() {
    let root = corpus();
    assert_eq!(
        root["payload_canonical_sha256"],
        "efa59a6b5efddada4a9904bf3bed7f70577ee8841db2d318d1a935786848f5ab"
    );
    assert_eq!(root["active_contract_cases"][0]["id"], "EA04");
}
#[test]
fn source_comparison_ex04_01_integer_beyond_binary64_precision() {
    receive("EX04-01");
}
#[test]
fn source_comparison_ex04_02_huge_integer_is_not_float_converted() {
    receive("EX04-02");
}
#[test]
fn source_comparison_ex04_03_decimal_comparison_does_not_round() {
    receive("EX04-03");
}
#[test]
fn source_comparison_ex04_04_scale_and_signed_zero() {
    receive("EX04-04");
}
#[test]
fn source_comparison_ex04_05_exact_decimal_binary64_difference() {
    receive("EX04-05");
}
#[test]
fn source_comparison_ex06_01_quiet_nan_ordering_and_equality() {
    receive("EX06-01");
}
#[test]
fn source_comparison_ex06_02_signaling_nan_equality() {
    receive("EX06-02");
}
#[test]
fn source_comparison_ex06_03_float_signal_precedes_nan() {
    receive("EX06-03");
}
#[test]
fn source_comparison_ea04_float_signal_survives_resource_refusal() {
    receive("EA04");
}
