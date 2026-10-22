//! Compatibility path: one neutral implementation owns JSON interpretation.
pub use crate::sha256 as digest;
pub use semantica_json::{JsonError, canonical_json, ordered_json, strict_json};
#[cfg(test)]
use serde_json::Value;

/// Existing raw-byte hashing remains the wire owner; JSON meaning/formatting is delegated.
pub fn canonical_digest(value: &serde_json::Value) -> Result<String, JsonError> {
    Ok(crate::sha256(canonical_json(value)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn duplicates_are_rejected_without_echoing_keys_at_any_depth() {
        for raw in [
            r#"{"private-field":1,"private-field":2}"#,
            r#"{"x":[{"secret":1,"\u0073ecret":2}]}"#,
            r#"{"$serde_json::private::Number":1,"$serde_json::private::Number":2}"#,
        ] {
            let error = strict_json(raw.as_bytes()).unwrap_err();
            assert_eq!(error, JsonError::DuplicateJsonKey);
            assert_eq!(error.to_string(), "duplicate_json_key");
        }
    }

    #[test]
    fn integer_precision_and_real_private_marker_objects_survive() {
        let raw = br#"{"large":1844674407370955161600000000000001,"negative":-1844674407370955161600000000000001,"object":{"$serde_json::private::Number":"1e400"},"raw":{"$serde_json::private::RawValue":"not JSON"}}"#;
        let value = strict_json(raw).unwrap();
        assert_eq!(
            value["large"].to_string(),
            "1844674407370955161600000000000001"
        );
        assert_eq!(
            value["negative"].to_string(),
            "-1844674407370955161600000000000001"
        );
        assert!(value["object"].is_object());
        assert_eq!(value["object"]["$serde_json::private::Number"], "1e400");
        assert_eq!(value["raw"]["$serde_json::private::RawValue"], "not JSON");
        assert_eq!(
            strict_json(&canonical_json(&value).unwrap()).unwrap(),
            value
        );
    }

    #[test]
    fn parse_preserves_order_while_snapshot_encoding_sorts_recursively() {
        let value =
            strict_json("{\"中文\":\"原页\\r\\n\\t\",\"z\":0,\"a\":{\"z\":2,\"a\":1}}".as_bytes())
                .unwrap();
        assert_eq!(
            value
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["中文", "z", "a"]
        );
        assert_eq!(
            String::from_utf8(canonical_json(&value).unwrap()).unwrap(),
            "{\"a\":{\"a\":1,\"z\":2},\"z\":0,\"中文\":\"原页\\r\\n\\t\"}"
        );
        assert_eq!(
            canonical_digest(&json!({"b":2,"a":1})).unwrap(),
            "43258cff783fe7036d8a43033f830adfc60ec037382473548ac742b888292777"
        );
    }

    #[test]
    fn malformed_nonfinite_and_overdeep_inputs_fail_closed() {
        for raw in [
            "NaN",
            "Infinity",
            "-Infinity",
            r#"{"x":NaN}"#,
            "{} trailing",
            "[1,]",
            "{",
            "",
        ] {
            assert!(strict_json(raw.as_bytes()).is_err());
        }
        for raw in ["1e400", "-1e400", "[1e400]", r#"{"x":1e400}"#] {
            assert_eq!(
                strict_json(raw.as_bytes()),
                Err(JsonError::NonfiniteJsonNumber)
            );
        }
        assert_eq!(strict_json(&[0xff]), Err(JsonError::MalformedJson));
        let deep = format!("{}0{}", "[".repeat(130), "]".repeat(130));
        assert!(strict_json(deep.as_bytes()).is_err());
        let mut value = json!(0);
        for _ in 0..130 {
            value = Value::Array(vec![value]);
        }
        assert_eq!(canonical_json(&value), Err(JsonError::NestingLimitExceeded));
    }

    #[test]
    fn canonical_numbers_retain_the_existing_python_v1_golden_values() {
        for (raw, expected) in [
            ("1e-7", "1e-07"),
            ("1e-6", "1e-06"),
            ("1e-5", "1e-05"),
            ("1e-4", "0.0001"),
            ("1e15", "1000000000000000.0"),
            ("1e16", "1e+16"),
            ("1e20", "1e+20"),
            ("1.23000", "1.23"),
            ("-0", "0"),
            ("-0.0", "-0.0"),
            ("1e-400", "0.0"),
            ("-1e-400", "-0.0"),
            ("5e-324", "5e-324"),
            ("1.7976931348623157e308", "1.7976931348623157e+308"),
            ("0.10000000000000001", "0.1"),
            (
                "123456789012345678901234567890",
                "123456789012345678901234567890",
            ),
        ] {
            let value = strict_json(raw.as_bytes()).unwrap();
            assert_eq!(
                String::from_utf8(canonical_json(&value).unwrap()).unwrap(),
                expected
            );
        }
        let nonfinite: Value = serde_json::from_str("1e400").unwrap();
        assert_eq!(
            canonical_json(&nonfinite),
            Err(JsonError::NonfiniteJsonNumber)
        );
    }

    #[test]
    fn noncanonical_external_float_values_still_receive_f64_semantics() {
        let external: Value = serde_json::from_str("0.10000000000000001").unwrap();
        assert_eq!(canonical_json(&external).unwrap(), b"0.1");
        let positive = strict_json(b"0.0").unwrap();
        let negative = strict_json(b"-0.0").unwrap();
        assert_ne!(
            canonical_digest(&positive).unwrap(),
            canonical_digest(&negative).unwrap()
        );
    }

    #[test]
    fn ordered_wire_keeps_recursive_insertion_order_and_python_float_spelling() {
        let value = strict_json(br#"{"z":1e-7,"a":{"z":1e16,"a":[1e-4,-0,-0.0]}}"#).unwrap();
        assert_eq!(
            ordered_json(&value).unwrap(),
            br#"{"z":1e-07,"a":{"z":1e+16,"a":[0.0001,0,-0.0]}}"#
        );
        assert_eq!(
            canonical_json(&value).unwrap(),
            br#"{"a":{"a":[0.0001,0,-0.0],"z":1e+16},"z":1e-07}"#
        );
        assert_eq!(strict_json(&ordered_json(&value).unwrap()).unwrap(), value);
        let changed_order = strict_json(br#"{"a":2,"z":1}"#).unwrap();
        let original_order = strict_json(br#"{"z":1,"a":2}"#).unwrap();
        assert_ne!(
            digest(ordered_json(&changed_order).unwrap()),
            digest(ordered_json(&original_order).unwrap())
        );
        assert_eq!(
            canonical_digest(&changed_order).unwrap(),
            canonical_digest(&original_order).unwrap()
        );
    }

    #[test]
    fn negative_integer_zero_normalizes_but_negative_float_zero_retains_sign() {
        let parsed = strict_json(b"-0").unwrap();
        assert_eq!(parsed.to_string(), "0");
        assert_eq!(ordered_json(&parsed).unwrap(), b"0");
        let external: Value = serde_json::from_str("-0").unwrap();
        assert_eq!(canonical_json(&external).unwrap(), b"0");
        assert_eq!(ordered_json(&external).unwrap(), b"0");
        for literal in ["-0.0", "-0e0", "-1e-400"] {
            let value = strict_json(literal.as_bytes()).unwrap();
            assert_eq!(ordered_json(&value).unwrap(), b"-0.0");
            assert_eq!(canonical_json(&value).unwrap(), b"-0.0");
        }
    }
}
