use semantica_json::{
    native_operations::{self as op, OperationError as E, TextLimits},
    native_value::{NativeLimits, NativeNode as N, NativeValue},
};
use serde_json::{Value, json};
fn limits() -> NativeLimits {
    NativeLimits {
        max_nodes: 100_000,
        max_links: 200_000,
        max_scalar_bytes: 1_000_000,
        max_depth: 4096,
        max_wire_bytes: 8_000_000,
    }
}
fn text_limits() -> TextLimits {
    TextLimits {
        max_codepoints: 1_000_000,
        max_visits: 200_000,
        max_depth: 4096,
    }
}
fn error(e: E) -> Value {
    json!({"status":"error","class":e.original_class().expect("unexpected native resource failure")})
}
#[test]
fn original_4432_native_operations_keep_text_bits_truth_and_error_categories() {
    let plan: Value = serde_json::from_slice(include_bytes!(
        "fixtures/original_native_operations_plan.json"
    ))
    .unwrap();
    let observations: Value = serde_json::from_slice(include_bytes!(
        "fixtures/original_native_operations_observations.json"
    ))
    .unwrap();
    let cases = plan["cases"].as_array().unwrap();
    let rows = observations["rows"].as_array().unwrap();
    assert_eq!(cases.len(), 1108);
    assert_eq!(rows.len(), cases.len());
    let mut differences = Vec::new();
    for (case, row) in cases.iter().zip(rows) {
        assert_eq!(case["id"], row["id"]);
        let value = NativeValue::from_envelope(&case["value"], limits()).unwrap();
        for name in ["str", "repr", "truth", "float"] {
            let actual = match name {
                "str" | "repr" => {
                    match op::python_text(&value, value.root(), name == "repr", text_limits()) {
                        Ok(s) => json!({"status":"ok","codepoints":s}),
                        Err(e) => error(e),
                    }
                }
                "truth" => match op::truth(&value, value.root()) {
                    Ok(v) => json!({"status":"ok","value":v}),
                    Err(e) => error(e),
                },
                _ => match op::builtin_float(&value, value.root()) {
                    Ok(v) => json!({"status":"ok","bits":format!("{:016x}",v.to_bits())}),
                    Err(e) => error(e),
                },
            };
            let mut expected = row["operations"][name].clone();
            if let Some(m) = expected.as_object_mut() {
                m.remove("message_codepoints");
            }
            if actual != expected {
                differences.push(
                    json!({"id":case["id"],"operation":name,"actual":actual,"expected":expected}),
                );
            }
        }
        // Operations must not normalize retained NaN bits, dict order or types.
        assert_eq!(value.to_envelope(limits()).unwrap(), case["value"]);
    }
    assert!(
        differences.is_empty(),
        "{} differences: {:?}",
        differences.len(),
        &differences[..differences.len().min(12)]
    );
}
#[test]
fn native_operation_expansion_depth_output_and_references_are_bounded() {
    let value = NativeValue::new(
        vec![N::Text(vec![0xd800]), N::List(vec![0; 10000])],
        1,
        limits(),
    )
    .unwrap();
    assert_eq!(
        op::python_text(
            &value,
            1,
            true,
            TextLimits {
                max_visits: 2,
                ..text_limits()
            }
        ),
        Err(E::VisitLimit)
    );
    assert_eq!(
        op::python_text(
            &value,
            0,
            true,
            TextLimits {
                max_codepoints: 7,
                ..text_limits()
            }
        ),
        Err(E::TextLimit)
    );
    assert_eq!(
        op::python_text(
            &value,
            0,
            false,
            TextLimits {
                max_codepoints: 1,
                ..text_limits()
            }
        )
        .unwrap(),
        vec![0xd800]
    );
    assert_eq!(
        op::python_text(
            &value,
            1,
            true,
            TextLimits {
                max_depth: 0,
                ..text_limits()
            }
        ),
        Err(E::DepthLimit)
    );
    assert_eq!(op::truth(&value, 2), Err(E::Reference));
    assert_eq!(op::builtin_float(&value, 2), Err(E::Reference));
    assert_eq!(
        op::python_text(&value, 2, true, text_limits()),
        Err(E::Reference)
    );
    assert_eq!(op::strip(&[0xa0, 32, 0xd800, 32, 0x2003]), &[0xd800]);
    assert_eq!(op::strip(&[0x1c, 32, 0x2003]), &[] as &[u32]);
}
#[test]
fn ordinary_string_repr_uses_the_same_quote_owner_without_changing_old_results() {
    for s in [
        "",
        "can't",
        "a\"b",
        "'\"\\\n\r\t",
        "\0\u{ad}\u{200b}\u{e000}\u{10ffff}",
        "机械😀",
    ] {
        let ordinary = json!(s);
        let native = NativeValue::from_plain_json(&ordinary, limits()).unwrap();
        let result = op::python_text(&native, native.root(), true, text_limits()).unwrap();
        assert_eq!(
            result,
            semantica_json::python_repr(&ordinary)
                .unwrap()
                .chars()
                .map(u32::from)
                .collect::<Vec<_>>()
        );
    }
}
