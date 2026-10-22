use semantica_contracts::json::{
    JsonError, canonical_json, ordered_json, protocol_json, strict_json,
};
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Deserialize)]
struct Containers {
    #[serde(deserialize_with = "semantica_contracts::json::deserialize_preserved")]
    values: Vec<Option<Value>>,
    #[serde(
        default,
        deserialize_with = "semantica_contracts::json::deserialize_preserved"
    )]
    optional: Option<Vec<Value>>,
}
#[test]
fn single_json_owner_preserves_both_explicit_profiles() {
    let raw = r#"{"$serde_json::private::Number":"42","nested":{"$serde_json::private::RawValue":"[true]"}}"#;
    for v in [
        strict_json(raw.as_bytes()).unwrap(),
        protocol_json(raw, raw.len()).unwrap(),
    ] {
        assert!(v.is_object());
        assert_eq!(v["$serde_json::private::Number"], "42");
        assert_eq!(v["nested"]["$serde_json::private::RawValue"], "[true]");
    }
    assert_eq!(
        strict_json(br#"{"k":1,"k":2}"#),
        Err(JsonError::DuplicateJsonKey)
    );
    assert_eq!(protocol_json(r#"{"k":1,"k":2}"#, 64).unwrap()["k"], 2);
    assert!(protocol_json(raw, raw.len() - 1).is_err());
    assert_eq!(strict_json(b"1e999"), Err(JsonError::NonfiniteJsonNumber));
    assert_eq!(
        protocol_json("1e999", 64),
        Err(JsonError::NonfiniteJsonNumber)
    );
    assert!(strict_json(b"{invalid").is_err());
    assert!(protocol_json("{invalid", 64).is_err());
}
#[test]
fn raw_value_hook_preserves_recursive_option_vec_containers_and_real_numbers() {
    let raw = r#"{"values":[{"$serde_json::private::Number":"42"},null,[1,{"$serde_json::private::RawValue":"true"}]],"optional":[1234567890123456789012345678901234567890]}"#;
    let value = protocol_json(raw, 4096).unwrap();
    for v in [
        serde_json::from_str::<Containers>(raw).unwrap(),
        serde_json::from_value::<Containers>(value).unwrap(),
    ] {
        assert!(v.values[0].as_ref().unwrap().is_object());
        assert!(v.values[1].is_none());
        assert_eq!(
            v.values[2].as_ref().unwrap()[1]["$serde_json::private::RawValue"],
            "true"
        );
        assert_eq!(
            v.optional.as_ref().unwrap()[0].to_string(),
            "1234567890123456789012345678901234567890"
        );
    }
    assert!(
        serde_json::from_str::<Containers>(r#"{"values":[]}"#)
            .unwrap()
            .optional
            .is_none()
    );
    assert!(serde_json::from_str::<Containers>(r#"{"values":{}}"#).is_err());
}
#[test]
fn canonical_and_ordered_formats_keep_existing_python_identity() {
    let v = strict_json(br#"{"z":1.0,"a":"x","i":-0}"#).unwrap();
    assert_eq!(canonical_json(&v).unwrap(), br#"{"a":"x","i":0,"z":1.0}"#);
    assert_eq!(ordered_json(&v).unwrap(), br#"{"z":1.0,"a":"x","i":0}"#);
    let v = json!({"$serde_json::private::Number":"42"});
    assert_eq!(
        canonical_json(&v).unwrap(),
        br#"{"$serde_json::private::Number":"42"}"#
    );
}

#[test]
fn dto_hook_keeps_original_number_profile_separate_from_finite_provider() {
    let raw = r#"{"values":[1e400,-1e400,-0,1.23000],"optional":null}"#;
    let input = serde_json::json!({"values":[serde_json::from_str::<Value>("1e400").unwrap(),serde_json::from_str::<Value>("-1e400").unwrap(),serde_json::from_str::<Value>("-0").unwrap(),serde_json::from_str::<Value>("1.23000").unwrap()],"optional":null});
    for value in [
        serde_json::from_str::<Containers>(raw).unwrap(),
        serde_json::from_value::<Containers>(input).unwrap(),
    ] {
        for (actual, expected) in value
            .values
            .iter()
            .zip(["1e+400", "-1e+400", "0", "1.23000"])
        {
            assert_eq!(actual.as_ref().unwrap().to_string(), expected);
        }
        assert!(value.optional.is_none());
    }
    assert_eq!(
        protocol_json("1e400", 64),
        Err(JsonError::NonfiniteJsonNumber)
    );
    assert_eq!(strict_json(b"1e400"), Err(JsonError::NonfiniteJsonNumber));
}
