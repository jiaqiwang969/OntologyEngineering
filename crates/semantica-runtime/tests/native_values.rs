use semantica_contracts::SemanticNativeValueEnvelope;
use semantica_json::native_value::{NativeLimits, NativeNode as N, NativeValue};
use semantica_runtime::native_values::{decode, decode_wire, encode};
use serde_json::json;
fn limits() -> NativeLimits {
    NativeLimits {
        max_nodes: 100,
        max_links: 100,
        max_scalar_bytes: 10000,
        max_depth: 32,
        max_wire_bytes: 100000,
    }
}
#[test]
fn generated_native_value_contract_preserves_special_values_and_refuses_wire_drift() {
    let value = NativeValue::new(
        vec![
            N::None,
            N::FloatBits(0x7ff8000000000042),
            N::Text(vec![0xd800]),
            N::Dict(vec![(0, 1), (2, 1)]),
        ],
        3,
        limits(),
    )
    .unwrap();
    let encoded = encode(&value, limits()).unwrap();
    assert!(encoded.validate().is_ok());
    let bytes = serde_json::to_vec(&encoded).unwrap();
    let received = decode_wire(&bytes, limits()).unwrap();
    let decoded = decode(&received, limits()).unwrap();
    assert_eq!(
        decoded.encode(limits()).unwrap(),
        value.encode(limits()).unwrap()
    );
    let mut wrong = serde_json::to_value(&encoded).unwrap();
    wrong["extra"] = json!("silently dropped?");
    assert!(decode_wire(&serde_json::to_vec(&wrong).unwrap(), limits()).is_err());
    let wrong = SemanticNativeValueEnvelope {
        schema: "wrong".into(),
        root: encoded.root,
        nodes: encoded.nodes,
    };
    assert!(decode(&wrong, limits()).is_err());
    let bad = SemanticNativeValueEnvelope {
        schema: "semantica.native-value/1".into(),
        root: 0,
        nodes: vec![json!({"kind":"list","items":[0]})],
    };
    assert!(decode(&bad, limits()).is_err());
}

#[test]
fn original_import_parser_reaches_generated_envelope_without_plain_json_loss() {
    use semantica_json::{
        original_json::{ParseError, ParseLimits},
        original_json_bytes::DecodeLimits,
    };
    use semantica_runtime::native_values::parse_import_json;
    let l = ParseLimits {
        decode: DecodeLimits {
            max_input_bytes: 10000,
            max_codepoints: 10000,
        },
        native: limits(),
    };
    let original: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../semantica-json/tests/fixtures/original_json_bytes_observations-01.json"
    ))
    .unwrap();
    for (id, bytes) in [
        ("nan", b"NaN".as_slice()),
        ("float-negative-zero", b"-0.0"),
        ("surrogate", br#""\ud800""#),
        ("duplicate", br#"{"z":1,"a":2,"z":3}"#),
    ] {
        let expected = original["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == id)
            .unwrap();
        let envelope = parse_import_json(bytes, l).unwrap();
        assert_eq!(
            serde_json::to_value(&envelope).unwrap(),
            expected["parsed"]["value"],
            "{id}"
        );
        let wire = serde_json::to_vec(&envelope).unwrap();
        let received = decode_wire(&wire, l.native).unwrap();
        assert_eq!(
            decode(&received, l.native)
                .unwrap()
                .to_envelope(l.native)
                .unwrap(),
            expected["parsed"]["value"]
        );
    }
    let error = parse_import_json(br#"{"a":}"#, l).unwrap_err();
    assert!(matches!(error.downcast_ref::<ParseError>(),Some(ParseError::Syntax(e)) if e.pos==5));
    let mut small = l;
    small.native.max_wire_bytes = 1;
    assert!(parse_import_json(b"NaN", small).is_err());
}
