use semantica_json::{
    native_operations::{OperationError, TextLimits},
    native_value::{NativeLimits, NativeValue},
};
use semantica_runtime::explorer_search_index::normalize_native;
use serde_json::{Value, json};
use std::time::{Duration, Instant};
fn limits() -> NativeLimits {
    NativeLimits {
        max_nodes: 10000,
        max_links: 10000,
        max_scalar_bytes: 1_000_000,
        max_depth: 1024,
        max_wire_bytes: 8_000_000,
    }
}
fn text_limits() -> TextLimits {
    TextLimits {
        max_codepoints: 1_000_000,
        max_visits: 10000,
        max_depth: 1024,
    }
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}
#[test]
fn original_1117_normalizations_preserve_surrogates_sigma_and_error_categories() {
    let plan: Value = serde_json::from_slice(include_bytes!(
        "fixtures/original_native_normalize_plan.json"
    ))
    .unwrap();
    let observations: Value = serde_json::from_slice(include_bytes!(
        "fixtures/original_native_normalize_observations.json"
    ))
    .unwrap();
    let cases = plan["cases"].as_array().unwrap();
    let rows = observations["rows"].as_array().unwrap();
    assert_eq!(cases.len(), 1117);
    assert_eq!(rows.len(), cases.len());
    let mut differences = Vec::new();
    for (case, row) in cases.iter().zip(rows) {
        assert_eq!(case["id"], row["id"]);
        let value = NativeValue::from_envelope(&case["value"], limits()).unwrap();
        let actual = match normalize_native(&value, value.root(), text_limits(), deadline()) {
            Ok(text) => json!({"status":"ok","codepoints":text}),
            Err(e) => {
                json!({"status":"error","class":e.downcast_ref::<OperationError>().unwrap().original_class().unwrap()})
            }
        };
        let mut expected = row["result"].clone();
        expected
            .as_object_mut()
            .unwrap()
            .remove("message_codepoints");
        if actual != expected {
            differences.push(json!({"id":case["id"],"actual":actual,"expected":expected}));
        }
    }
    assert!(
        differences.is_empty(),
        "{} differences: {:?}",
        differences.len(),
        &differences[..differences.len().min(12)]
    );
}
#[test]
fn native_normalization_keeps_host_limits_and_expired_deadlines() {
    let value = NativeValue::from_plain_json(&json!("İ"), limits()).unwrap();
    assert!(
        normalize_native(
            &value,
            value.root(),
            text_limits(),
            Instant::now() - Duration::from_secs(1)
        )
        .is_err()
    );
    assert_eq!(
        normalize_native(
            &value,
            value.root(),
            TextLimits {
                max_codepoints: 1,
                ..text_limits()
            },
            deadline()
        )
        .unwrap_err()
        .to_string(),
        "explorer_native_text_limit"
    );
    assert_eq!(
        normalize_native(&value, value.root(), text_limits(), deadline()).unwrap(),
        vec![105, 0x307]
    );
    assert!(normalize_native(&value, 999, text_limits(), deadline()).is_err());
}
