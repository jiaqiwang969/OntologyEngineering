use semantica_runtime::schema::RustSchemaRuntime;
use serde_json::Value;

#[test]
fn production_registered_strong_boundary_consumes_all_original_validated_model_cases() {
    let runtime = RustSchemaRuntime::new().unwrap();
    let evidence = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let mut models = std::collections::BTreeSet::new();
    let mut consumed = 0;
    for file in [
        "ORIGINAL106-FROZEN-EXPECTATIONS.json",
        "ORIGINAL106-EXTENDED-FROZEN.json",
        "ORIGINAL106-SCALAR-BOUNDARIES-FROZEN.json",
    ] {
        let frozen: Value =
            serde_json::from_slice(&std::fs::read(evidence.join(file)).unwrap()).unwrap();
        for case in frozen["cases"].as_array().unwrap() {
            if case["native_nonfinite_profile"] == true {
                continue;
            }
            let id = case["model"].as_str().unwrap();
            let result = runtime.validate_snapshot(id, &case["input"]);
            if case["original"]["status"] == "validated" {
                let snapshot = result.unwrap();
                assert_eq!(snapshot.model_id(), id);
                assert_eq!(
                    serde_json::to_value(snapshot).unwrap(),
                    case["original"]["output"],
                    "{}",
                    case["id"]
                );
                models.insert(id.to_owned());
                consumed += 1;
            } else {
                assert_eq!(
                    result.unwrap_err().code,
                    "schema_model_validation_failed",
                    "{}",
                    case["id"]
                );
            }
        }
    }
    assert_eq!(models.len(), 106);
    assert_eq!(consumed, 2222);
    assert_eq!(
        runtime
            .validate_snapshot("unregistered", &serde_json::json!({}))
            .unwrap_err()
            .code,
        "schema_model_unknown"
    );
}

#[test]
fn additional_original_numeric_cases_cross_the_same_strong_boundary() {
    let runtime = RustSchemaRuntime::new().unwrap();
    let evidence = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let mut consumed = 0;
    for file in [
        "ORIGINAL106-UNICODE-NUMERIC-FROZEN.json",
        "ORIGINAL106-HUGE-NUMERIC-FROZEN.json",
    ] {
        let frozen: Value =
            serde_json::from_slice(&std::fs::read(evidence.join(file)).unwrap()).unwrap();
        for case in frozen["cases"].as_array().unwrap() {
            let result = runtime.validate_snapshot(case["model"].as_str().unwrap(), &case["input"]);
            if case["native_nonfinite_profile"] == true {
                assert_eq!(
                    result.unwrap_err().code,
                    "schema_nonfinite_profile_unavailable"
                );
            } else if case["original"]["status"] == "raised" {
                assert_eq!(result.unwrap_err().code, "schema_python_float_overflow");
            } else if case["original"]["status"] == "validated" {
                assert_eq!(
                    serde_json::to_value(result.unwrap()).unwrap(),
                    case["original"]["output"]
                );
                consumed += 1;
            } else {
                assert_eq!(result.unwrap_err().code, "schema_model_validation_failed");
            }
        }
    }
    assert_eq!(consumed, 613);
}
