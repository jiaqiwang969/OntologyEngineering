//! Characterisation of `semantica/utils/validators.py` (expected values from the oracle).

mod common;

use common::{err_json, obj, oracle};
use indexmap::IndexMap;
use sem_utils::types::{Entity, Relationship};
use sem_utils::validators::{
    ConfigOptions, Constraints, PyType, validate_config, validate_data, validate_email,
    validate_entity, validate_file_path, validate_list_constraints, validate_numeric_constraints,
    validate_relationship, validate_required_fields, validate_schema, validate_string_constraints,
    validate_types, validate_url,
};
use serde_json::{Number, Value, json};

fn py_type(name: &str) -> PyType {
    match name {
        "str" => PyType::Str,
        "int" => PyType::Int,
        "float" => PyType::Float,
        "bool" => PyType::Bool,
        "list" => PyType::List,
        "dict" => PyType::Dict,
        "NoneType" => PyType::NoneType,
        "object" => PyType::Object,
        other => panic!("unknown type {other}"),
    }
}

fn type_map(value: &Value) -> IndexMap<String, PyType> {
    value
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), py_type(v.as_str().unwrap())))
        .collect()
}

fn number(value: &Value) -> Option<Number> {
    value.as_number().cloned()
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect()
}

fn constraints(c: &Value) -> Constraints {
    Constraints {
        required_fields: c.get("required_fields").map(strings),
        field_types: c.get("field_types").map(type_map),
        min_length: c.get("min_length").and_then(number),
        max_length: c.get("max_length").and_then(number),
        min_value: c.get("min_value").and_then(number),
        max_value: c.get("max_value").and_then(number),
        pattern: c.get("pattern").and_then(Value::as_str).map(str::to_string),
        allowed_values: c.get("allowed_values").and_then(Value::as_array).cloned(),
        item_type: c.get("item_type").and_then(Value::as_str).map(py_type),
        unique: c.get("unique").and_then(Value::as_bool).unwrap_or(false),
    }
}

fn tuple_json(result: (bool, Option<String>)) -> Value {
    json!([result.0, result.1])
}

fn outcome(result: Result<(bool, Option<String>), sem_utils::SemanticaError>) -> Value {
    match result {
        Ok(r) => json!({"ok": tuple_json(r)}),
        Err(e) => err_json(&e),
    }
}

fn expected(case: &Value) -> Value {
    common::expected_outcome(case)
}

#[test]
fn test_validate_data_matches_oracle() {
    for case in oracle("validate_data").as_array().unwrap() {
        let schema = case["schema"].as_object();
        let got = outcome(validate_data(
            &case["data"],
            schema,
            &constraints(&case["constraints"]),
        ));
        assert_eq!(got, expected(case), "{case}");
    }
}

#[test]
fn test_validate_config_matches_oracle() {
    for case in oracle("validate_config").as_array().unwrap() {
        let o = &case["options"];
        let options = ConfigOptions {
            allowed_keys: o.get("allowed_keys").map(strings),
            key_types: o.get("key_types").map(type_map),
            nested_validation: o.get("nested_validation").and_then(Value::as_bool),
        };
        let required: Option<Vec<String>> = case["required_keys"]
            .as_array()
            .map(|_| strings(&case["required_keys"]));
        let required_refs: Option<Vec<&str>> = required
            .as_ref()
            .map(|r| r.iter().map(String::as_str).collect());
        let got = validate_config(&case["config"], required_refs.as_deref(), &options);
        assert_eq!(json!({"ok": tuple_json(got)}), expected(case), "{case}");
    }
}

#[test]
fn test_validate_entity_and_relationship_match_oracle() {
    for case in oracle("validate_entity").as_array().unwrap() {
        let entity = obj(&case[0]);
        assert_eq!(tuple_json(validate_entity(&entity)), case[1], "{case}");
    }
    let mut entity = Entity::new("e", "t", "x");
    entity.confidence = 7.0;
    assert_eq!(
        tuple_json(validate_entity(&entity)),
        *oracle("validate_entity_object")
    );
    for case in oracle("validate_relationship").as_array().unwrap() {
        let rel = obj(&case[0]);
        assert_eq!(tuple_json(validate_relationship(&rel)), case[1], "{case}");
    }
    let mut rel = Relationship::new("r", "a", "b", "t");
    rel.confidence = 9.0;
    assert_eq!(
        tuple_json(validate_relationship(&rel)),
        *oracle("validate_relationship_object")
    );
}

#[test]
fn test_validate_url_and_email_match_oracle() {
    for case in oracle("validate_url").as_array().unwrap() {
        let url = case[0].as_str().unwrap();
        assert_eq!(tuple_json(validate_url(url)), case[1], "{url:?}");
    }
    for case in oracle("validate_email").as_array().unwrap() {
        let email = case[0].as_str().unwrap();
        assert_eq!(tuple_json(validate_email(email)), case[1], "{email:?}");
    }
}

#[test]
fn test_validate_file_path_matches_oracle() {
    for case in oracle("validate_file_path").as_array().unwrap() {
        let path = case[0].as_str().unwrap();
        let must_exist = case[1].as_bool().unwrap();
        let allowed: Option<Vec<String>> = case[2].as_array().map(|_| strings(&case[2]));
        let allowed_refs: Option<Vec<&str>> = allowed
            .as_ref()
            .map(|a| a.iter().map(String::as_str).collect());
        let got = validate_file_path(path, Some(must_exist), allowed_refs.as_deref());
        assert_eq!(tuple_json(got), case[3], "{case}");
    }
    let tmp = tempfile::NamedTempFile::new().unwrap();
    assert_eq!(
        validate_file_path(tmp.path(), Some(true), None),
        (true, None)
    );
}

#[test]
fn test_single_constraint_validators_match_oracle() {
    let c = Constraints {
        min_length: Some(1.into()),
        max_length: Some(5.into()),
        pattern: Some("a".into()),
        allowed_values: Some(vec![json!("abc")]),
        ..Default::default()
    };
    assert_eq!(
        Value::Array(vec![tuple_json(
            validate_string_constraints("abc", &c).unwrap()
        )]),
        *oracle("validate_string_constraints")
    );
    let n = Constraints {
        min_value: Some(1.into()),
        max_value: Some(10.into()),
        ..Default::default()
    };
    let expected = oracle("validate_numeric_constraints").as_array().unwrap();
    assert_eq!(
        tuple_json(validate_numeric_constraints(&json!(7), &n).unwrap()),
        expected[0]
    );
    let min_only = Constraints {
        min_value: Some(1.into()),
        ..Default::default()
    };
    assert_eq!(
        outcome(validate_numeric_constraints(&json!("7"), &min_only)),
        expected[1]
    );
    let l = Constraints {
        min_length: Some(1.into()),
        item_type: Some(PyType::Int),
        unique: true,
        ..Default::default()
    };
    assert_eq!(
        Value::Array(vec![tuple_json(
            validate_list_constraints(&[json!(1), json!(2)], &l).unwrap()
        )]),
        *oracle("validate_list_constraints")
    );
    let types = IndexMap::from([
        ("a".to_string(), PyType::Int),
        ("b".to_string(), PyType::Int),
        ("c".to_string(), PyType::Str),
    ]);
    assert_eq!(
        Value::Array(vec![tuple_json(validate_types(
            &obj(&json!({"a": 1, "b": "x"})),
            &types
        ))]),
        *oracle("validate_types")
    );
    assert_eq!(
        Value::Array(vec![tuple_json(validate_required_fields(
            &obj(&json!({"a": 1})),
            &["a", "b", "c"]
        ))]),
        *oracle("validate_required_fields")
    );
    assert_eq!(
        Value::Array(vec![tuple_json(
            validate_schema(
                &json!({"a": 1}),
                &obj(&json!({"type": "object", "required": ["a"]}))
            )
            .unwrap()
        )]),
        *oracle("validate_schema")
    );
    // An invalid pattern is an `re.error`.
    let bad = Constraints {
        pattern: Some("(".into()),
        ..Default::default()
    };
    let err = validate_string_constraints("x", &bad).unwrap_err();
    assert_eq!(err.type_name(), "error");
    assert_eq!(err.module_name(), "re");
    assert!(PyType::Object.is_instance(&Value::Null));
    assert_eq!(PyType::NoneType.name(), "NoneType");
}
