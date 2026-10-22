use semantica_json::{python_character_class, python_repr, python_str, python_strip, strict_json};
use sha2::{Digest, Sha256};

#[path = "../../../build_support/private_test_fixtures.rs"]
mod private_test_fixtures;

fn private_fixture_sha256(bytes: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(bytes.as_ref()))
}

#[test]
fn unicode_classifications_match_all_original_python_scalar_slots() {
    let original = private_test_fixtures::bytes("ORIGINAL-PYTHON-UNICODE-CLASSIFICATION.bin");
    assert_eq!(original.len(), 0x110000);
    let mut executed = 0;
    for (code, expected) in original.iter().enumerate() {
        if let Some(c) = char::from_u32(code as u32) {
            let (printable, whitespace) = python_character_class(c);
            assert_eq!(
                (u8::from(whitespace) << 1) | u8::from(printable),
                *expected,
                "U+{code:04X}"
            );
            executed += 1;
        }
    }
    assert_eq!(executed, 0x110000 - 0x800);
}

#[test]
fn ordered_native_text_and_escaped_representations_share_numeric_owner() {
    let object = strict_json(br#"{"z":"\u0007\u001b","a":1e16}"#).unwrap();
    assert_eq!(
        python_str(&object).unwrap(),
        "{'z': '\\x07\\x1b', 'a': 1e+16}"
    );
    let array = strict_json(br#"[1e16,-0.0,"\u200b"]"#).unwrap();
    assert_eq!(python_repr(&array).unwrap(), "[1e+16, -0.0, '\\u200b']");
    assert_eq!(python_strip("\u{1c} Alice \u{1f}"), "Alice");
    assert_eq!(
        python_repr(&serde_json::json!("can't")).unwrap(),
        "\"can't\""
    );
}

#[test]
fn custom_float_recognizes_every_original_unicode_decimal_digit() {
    let original: serde_json::Value = serde_json::from_slice(&private_test_fixtures::bytes(
        "ORIGINAL-PYTHON-UNICODE-DECIMAL.json",
    ))
    .unwrap();
    assert_eq!(original["values"].as_array().unwrap().len(), 680);
    for row in original["values"].as_array().unwrap() {
        let c = char::from_u32(row[0].as_u64().unwrap() as u32).unwrap();
        let result =
            semantica_json::python_builtin_float(&serde_json::json!(format!("{c}.5"))).unwrap();
        assert_eq!(result, row[1].as_u64().unwrap() as f64 + 0.5);
    }
}
