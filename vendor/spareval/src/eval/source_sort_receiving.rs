//! Independent receiving of frozen native CPython generic-key sort schedules.
//! Expected arrays mechanically project the original observation, not candidate
//! output. Production keys/query startup are received separately; this module
//! calls only the real index sorter and does not implement a sorting algorithm.
use super::source_sort::{SourceSortError, source_sort_indices};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::rc::Rc;

const FIXTURE: &str = include_str!("fixtures/source_sort_schedule_cases.json");
fn canonical(value: &Value) -> Value {
    match value {
        Value::Object(values) => Value::Object(
            values
                .iter()
                .collect::<BTreeMap<_, _>>()
                .into_iter()
                .map(|(key, value)| (key.clone(), canonical(value)))
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.iter().map(canonical).collect()),
        value => value.clone(),
    }
}
fn fixture() -> Value {
    assert_eq!(
        format!("{:x}", Sha256::digest(FIXTURE.as_bytes())),
        "1dc5d0db32c5cdedca42b4662e3197efa1675c4ca04b35397d2989f9ee5b6511"
    );
    let root: Value = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(
        root["source_record_sha256"],
        "533a4a9aaf381db47da90ff1252ad442e817c0eb22a9ff01250b5dacfc5ba8ea"
    );
    assert_eq!(
        root["input_record_sha256"],
        "af3b0d135e8881e504c131391153ff6a6b202228586f50a3f41adecf05d386c3"
    );
    assert_eq!(
        root["raw_observation_sha256"],
        "1fade9b9cfd4d1cf8948eb0190d8affd1b15d3122288913d168e239918a7205a"
    );
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&canonical(&root["cases"])).unwrap())
        ),
        "770a7d00b5e50fa0561132adea294c74fd6d1e9adce1d413a257e2f5e6743ab8"
    );
    root
}
#[derive(Debug)]
struct ComparisonMarker;
fn receive(id: &str) {
    let root = fixture();
    let case = root["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .unwrap();
    let values: Vec<i64> = case["input"]["values"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_i64().unwrap())
        .collect();
    let mut indices: Vec<usize> = (0..values.len()).collect();
    let mut comparisons = Vec::new();
    let marker = Rc::new(ComparisonMarker);
    let fail_at = case["input"]["fail_at_comparison"]
        .as_u64()
        .map(|v| v as usize);
    let result = source_sort_indices(
        &mut indices,
        case["input"]["descending"].as_bool().unwrap(),
        |a, b| {
            assert!(
                a < values.len() && b < values.len(),
                "{id}: invalid original index"
            );
            if Some(comparisons.len() + 1) == fail_at {
                comparisons.push(json!([a, b, "raised_marker"]));
                return Err(Rc::clone(&marker));
            }
            let less = values[a] < values[b];
            comparisons.push(json!([a, b, less]));
            Ok(less)
        },
        || Ok(()),
    );
    assert_eq!(
        json!(comparisons),
        case["expected"]["comparisons"],
        "{id}: exact comparator schedule"
    );
    match case["expected"]["outcome"].as_str().unwrap() {
        "success" => {
            assert!(result.is_ok(), "{id}: {result:?}");
            assert_eq!(
                json!(indices),
                case["expected"]["permutation"],
                "{id}: stable permutation"
            );
        }
        "raised_marker" => {
            match result {
                Err(SourceSortError::Callback(actual)) => {
                    assert!(Rc::ptr_eq(&actual, &marker), "{id}: error identity changed")
                }
                other => panic!("{id}: wrong failure {other:?}"),
            }
            assert_eq!(comparisons.len(), fail_at.unwrap());
            assert_eq!(case["expected"]["same_error_identity"], true);
            // Failure-time private permutation is intentionally not an output contract.
        }
        other => panic!("unknown frozen outcome {other}"),
    }
}

#[test]
fn source_order_sort_fixture_identity() {
    let root = fixture();
    let cases = root["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 25);
    let mut pairs = 0;
    let mut failed = 0;
    for (offset, case) in cases.iter().enumerate() {
        assert_eq!(case["id"], format!("ST{:02}", offset + 1));
        let len = case["input"]["values"].as_array().unwrap().len();
        assert_eq!(
            case["expected"]["key_order"],
            json!((0..len).collect::<Vec<_>>())
        );
        pairs += case["expected"]["comparisons"].as_array().unwrap().len();
        failed += usize::from(case["expected"]["outcome"] == "raised_marker");
    }
    assert_eq!((pairs, failed), (16007, 3));
}

#[test]
fn source_order_sort_st01() {
    receive("ST01");
}

#[test]
fn source_order_sort_st02() {
    receive("ST02");
}

#[test]
fn source_order_sort_st03() {
    receive("ST03");
}

#[test]
fn source_order_sort_st04() {
    receive("ST04");
}

#[test]
fn source_order_sort_st05() {
    receive("ST05");
}

#[test]
fn source_order_sort_st06() {
    receive("ST06");
}

#[test]
fn source_order_sort_st07() {
    receive("ST07");
}

#[test]
fn source_order_sort_st08() {
    receive("ST08");
}

#[test]
fn source_order_sort_st09() {
    receive("ST09");
}

#[test]
fn source_order_sort_st10() {
    receive("ST10");
}

#[test]
fn source_order_sort_st11() {
    receive("ST11");
}

#[test]
fn source_order_sort_st12() {
    receive("ST12");
}

#[test]
fn source_order_sort_st13() {
    receive("ST13");
}

#[test]
fn source_order_sort_st14() {
    receive("ST14");
}

#[test]
fn source_order_sort_st15() {
    receive("ST15");
}

#[test]
fn source_order_sort_st16() {
    receive("ST16");
}

#[test]
fn source_order_sort_st17() {
    receive("ST17");
}

#[test]
fn source_order_sort_st18() {
    receive("ST18");
}

#[test]
fn source_order_sort_st19() {
    receive("ST19");
}

#[test]
fn source_order_sort_st20() {
    receive("ST20");
}

#[test]
fn source_order_sort_st21() {
    receive("ST21");
}

#[test]
fn source_order_sort_st22() {
    receive("ST22");
}

#[test]
fn source_order_sort_st23() {
    receive("ST23");
}

#[test]
fn source_order_sort_st24() {
    receive("ST24");
}

#[test]
fn source_order_sort_st25() {
    receive("ST25");
}
