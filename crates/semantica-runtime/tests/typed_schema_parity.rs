use semantica_runtime::schema::RustSchemaRuntime;
use serde_json::Value;

#[test]
fn all_original_106_models_keep_frozen_validation_output_alias_and_issue_locations() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../ORIGINAL106-FROZEN-EXPECTATIONS.json");
    let frozen: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(frozen["model_count"], 106);
    assert_eq!(frozen["field_count"], 529);
    assert_eq!(frozen["annotations"].as_array().unwrap().len(), 53);
    let runtime = RustSchemaRuntime::new().unwrap();
    assert_eq!(runtime.model_ids().count(), 106);
    for case in frozen["cases"].as_array().unwrap() {
        let actual = runtime
            .validate_model(case["model"].as_str().unwrap(), &case["input"])
            .unwrap();
        let expected = &case["original"];
        assert_eq!(actual["status"], expected["status"], "{}", case["id"]);
        if expected["status"] == "validated" {
            assert_eq!(actual["output"], expected["output"], "{}", case["id"]);
            assert_eq!(
                actual["output_by_alias"], expected["output_by_alias"],
                "{}",
                case["id"]
            );
            assert_eq!(
                actual["properties"], expected["properties"],
                "{}",
                case["id"]
            );
        } else {
            let locations = |value: &Value| -> Vec<Value> {
                value["issues"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| x["loc"].clone())
                    .collect()
            };
            assert_eq!(locations(&actual), locations(expected), "{}", case["id"]);
        }
        assert_eq!(
            actual["input_after"], expected["input_after"],
            "{}",
            case["id"]
        );
        assert_eq!(
            actual["input_mutated"], expected["input_mutated"],
            "{}",
            case["id"]
        );
    }
}
