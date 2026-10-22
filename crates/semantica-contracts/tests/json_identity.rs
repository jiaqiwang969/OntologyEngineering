use semantica_contracts::{SemanticMcpRequest, SemanticPackageInput};
use serde_json::{Value, json};

fn marker() -> Value {
    json!({"$serde_json::private::Number":"42"})
}
fn envelope(value: Value) -> Value {
    json!({"schema":"semantica.mcp-service-request/1","expected_revision":0,"request":value})
}
fn preserved(value: &Value) {
    assert!(
        value.is_object(),
        "a literal private-number object must remain an object"
    );
    assert_eq!(value["$serde_json::private::Number"], "42");
}
#[test]
fn generated_value_from_str_preserves_literal_object_and_nested_arrays() {
    let raw = r#"{"schema":"semantica.mcp-service-request/1","expected_revision":0,"request":{"$serde_json::private::Number":"42"}}"#;
    let actual: SemanticMcpRequest = serde_json::from_str(raw).unwrap();
    preserved(&actual.request);
    preserved(
        &serde_json::from_slice::<SemanticMcpRequest>(raw.as_bytes())
            .unwrap()
            .request,
    );
    preserved(
        &serde_json::from_reader::<_, SemanticMcpRequest>(std::io::Cursor::new(raw))
            .unwrap()
            .request,
    );
    let actual: SemanticMcpRequest = serde_json::from_str(r#"{"schema":"semantica.mcp-service-request/1","expected_revision":0,"request":{"nested":[{"$serde_json::private::Number":"42"}]}}"#).unwrap();
    preserved(&actual.request["nested"][0]);
}
#[test]
fn generated_value_from_value_and_round_trip_preserve_the_same_object() {
    let original = envelope(marker());
    assert!(original["request"].is_object());
    let actual: SemanticMcpRequest = serde_json::from_value(original).unwrap();
    preserved(&actual.request);
    let encoded = serde_json::to_string(&actual).unwrap();
    preserved(
        &serde_json::from_str::<SemanticMcpRequest>(&encoded)
            .unwrap()
            .request,
    );
}
#[test]
fn generated_optional_value_keeps_object_null_and_absent_distinct_from_wrong_type() {
    let make = |manifest| json!({"schema":"semantica.package-input/1","inline_bytes":{},"manifest":manifest});
    let actual: SemanticPackageInput = serde_json::from_value(make(marker())).unwrap();
    preserved(actual.manifest.as_ref().unwrap());
    let actual: SemanticPackageInput = serde_json::from_str(r#"{"schema":"semantica.package-input/1","inline_bytes":{},"manifest":{"$serde_json::private::Number":"42"}}"#).unwrap();
    preserved(actual.manifest.as_ref().unwrap());
    assert!(
        serde_json::from_value::<SemanticPackageInput>(make(Value::Null))
            .unwrap()
            .manifest
            .is_none()
    );
    assert!(
        serde_json::from_str::<SemanticPackageInput>(
            r#"{"schema":"semantica.package-input/1","inline_bytes":{}}"#
        )
        .unwrap()
        .manifest
        .is_none()
    );
}
#[test]
fn generated_values_keep_real_numeric_precision_and_strict_outer_fields() {
    let n = "12345678901234567890123456789012345678901234567890";
    let raw = format!(
        r#"{{"schema":"semantica.mcp-service-request/1","expected_revision":0,"request":[{n},1.25,true,null,"42"]}}"#
    );
    let actual: SemanticMcpRequest = serde_json::from_str(&raw).unwrap();
    assert!(actual.request[0].is_number());
    assert_eq!(actual.request[0].to_string(), n);
    assert_eq!(actual.request[1].as_f64(), Some(1.25));
    assert_eq!(actual.request[2], true);
    assert!(actual.request[3].is_null());
    assert_eq!(actual.request[4], "42");
    let round: SemanticMcpRequest =
        serde_json::from_value(serde_json::to_value(&actual).unwrap()).unwrap();
    assert_eq!(round.request, actual.request);
    assert!(serde_json::from_str::<SemanticMcpRequest>(r#"{"schema":"semantica.mcp-service-request/1","expected_revision":0,"request":null,"passed":true}"#).is_err());
    assert!(
        serde_json::from_str::<SemanticMcpRequest>(
            r#"{"schema":"semantica.mcp-service-request/1","expected_revision":0}"#
        )
        .is_err()
    );
}
#[test]
fn generated_optional_json_absence_and_null_keep_original_defaults() {
    for raw in [
        r#"{"schema":"semantica.package-input/1","inline_bytes":{}}"#,
        r#"{"schema":"semantica.package-input/1","inline_bytes":{},"manifest":null}"#,
    ] {
        let value: SemanticPackageInput = serde_json::from_str(raw).unwrap();
        assert!(value.manifest.is_none());
    }
}

#[test]
fn generated_identity_retains_original_arbitrary_number_tokens() {
    for (token, expected) in [
        ("1e400", "1e+400"),
        ("-1e400", "-1e+400"),
        ("-0", "0"),
        ("1.23000", "1.23000"),
    ] {
        let raw = format!(
            r#"{{"schema":"semantica.mcp-service-request/1","expected_revision":0,"request":{token}}}"#
        );
        let actual: SemanticMcpRequest = serde_json::from_str(&raw).unwrap();
        assert!(actual.request.is_number());
        assert_eq!(actual.request.to_string(), expected);
        let from_value: SemanticMcpRequest =
            serde_json::from_value(serde_json::to_value(&actual).unwrap()).unwrap();
        assert_eq!(from_value.request.to_string(), expected);
    }
}
