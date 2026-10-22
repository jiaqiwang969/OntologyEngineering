use semantica_runtime::schema::RustSchemaRuntime;
use serde_json::Value;
#[test]
fn original21_schema_cases_keep_alias_default_constraints_and_properties() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../frozen-original-inputs/ACTUAL-PYDANTIC-ORACLE.json");
    let frozen: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(frozen["cases"].as_array().unwrap().len(), 21);
    let runtime = RustSchemaRuntime::new().unwrap();
    for case in frozen["cases"].as_array().unwrap() {
        let id = format!(
            "semantica.semantic_extract.schemas::{}",
            case["schema"].as_str().unwrap()
        );
        let result = runtime.validate_model(&id, &case["input_before"]).unwrap();
        assert_eq!(result["status"], case["status"], "{}", case["case"]);
        if case["status"] == "validated" {
            assert_eq!(result["output"], case["dump"], "{}", case["case"]);
            assert_eq!(
                result["output_by_alias"], case["dump_by_alias"],
                "{}",
                case["case"]
            );
            if let Some(properties) = case.get("relation_property_aliases") {
                assert_eq!(&result["properties"], properties);
            }
        }
        assert_eq!(result["input_after"], case["input_after"]);
        assert_eq!(result["input_mutated"], case["input_mutated"]);
    }
    // These native float observations stay retained as original data, not
    // translated into finite JSON compatibility or claimed as passed parity.
    assert_eq!(
        frozen["nonfinite_python_float_cases"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
}
#[test]
fn all_original_model_schemas_and_default_factories_remain_independent() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../ORIGINAL106-FROZEN-EXPECTATIONS.json");
    let frozen: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let runtime = RustSchemaRuntime::new().unwrap();
    for model in frozen["models"].as_array().unwrap() {
        assert_eq!(
            runtime
                .model_json_schema(model["id"].as_str().unwrap())
                .unwrap(),
            model["json_schema"]
        );
    }
    let id = "semantica.semantic_extract.schemas::EntitiesResponse";
    let input = serde_json::json!({});
    let mut first = runtime.validate_model(id, &input).unwrap();
    first["output"]["entities"] = serde_json::json!(["changed"]);
    assert_eq!(
        runtime.validate_model(id, &input).unwrap()["output"]["entities"],
        serde_json::json!([])
    );
}
