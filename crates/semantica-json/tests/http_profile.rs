use semantica_json::{JsonError, protocol_json, python_finite_json, python_str, strict_json};
use serde_json::json;
#[test]
fn finite_http_numbers_do_not_change_existing_json_profiles() {
    let text = r#"{"query":"old","query":"new","float":0.10000000000000000000000000000000000001,"integer_zero":-0,"float_zero":-0.0,"large":99999999999999999999999999999999999999999}"#;
    let http = python_finite_json(text, text.len()).unwrap();
    assert_eq!(http["query"], "new");
    assert_eq!(http["float"], json!(0.1));
    assert_eq!(python_str(&http["integer_zero"]).unwrap(), "0");
    assert_eq!(python_str(&http["float_zero"]).unwrap(), "-0.0");
    assert_eq!(
        http["large"].to_string(),
        "99999999999999999999999999999999999999999"
    );
    assert_eq!(
        strict_json(text.as_bytes()).unwrap_err(),
        JsonError::DuplicateJsonKey
    );
    let protocol = protocol_json(text, text.len()).unwrap();
    assert_eq!(
        protocol["float"].to_string(),
        "0.10000000000000000000000000000000000001"
    );
}
#[test]
fn literal_private_markers_remain_objects_and_limits_are_explicit() {
    let text = r#"{"$serde_json::private::Number":"100","nested":{"$serde_json::private::RawValue":"null"}}"#;
    let value = python_finite_json(text, text.len()).unwrap();
    assert!(value.is_object());
    assert_eq!(value["$serde_json::private::Number"], "100");
    assert!(value["nested"].is_object());
    assert!(python_finite_json(text, text.len() - 1).is_err());
    assert_eq!(
        python_finite_json("1e400", 100).unwrap_err(),
        JsonError::NonfiniteJsonNumber
    );
    let deep = format!("{}0{}", "[".repeat(300), "]".repeat(300));
    assert!(python_finite_json(&deep, deep.len()).is_err());
}
