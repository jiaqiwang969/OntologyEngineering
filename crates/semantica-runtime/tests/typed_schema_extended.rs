use semantica_runtime::schema::RustSchemaRuntime;
use serde_json::{Value, json};

fn compare(runtime: &RustSchemaRuntime, case: &Value) -> Vec<Value> {
    let result = match case["operation"].as_str().unwrap_or("model_validate") {
        "model_construct" => {
            runtime.construct_model(case["model"].as_str().unwrap(), &case["input"])
        }
        "constructor" => {
            runtime.validate_constructor(case["model"].as_str().unwrap(), &case["input"])
        }
        _ => runtime.validate_model(case["model"].as_str().unwrap(), &case["input"]),
    };
    let mut diffs = vec![];
    let mut actual = match result {
        Ok(x) => x,
        Err(x) => {
            diffs.push(json!({"id":case["id"],"failure":x}));
            return diffs;
        }
    };
    let expected = &case["original"];
    // Original snapshot reads properties after model_construct. Missing required
    // fields construct successfully, then original RelationOut.source raises.
    // Check the same property operation separately rather than labeling
    // unchecked construction as validation.
    if expected["exception"] == "AttributeError" {
        let failure = runtime
            .property_on_output(case["model"].as_str().unwrap(), &actual["output"], "source")
            .unwrap_err();
        assert_eq!(failure.code, "schema_property_field_missing");
        actual["status"] = json!("error");
    }
    let mut same = |key: &str| {
        if actual[key] != expected[key] {
            diffs.push(json!({"id":case["id"],"property":key,"actual":actual[key],"expected":expected[key]}));
        }
    };
    same("status");
    same("input_after");
    same("input_mutated");
    if expected["status"] == "error" {
        let signature = |v: &Value| {
            v["issues"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| json!({"type":x["type"],"loc":x["loc"]}))
                .collect::<Vec<_>>()
        };
        if signature(&actual) != signature(expected) {
            diffs.push(json!({"id":case["id"],"property":"issues_kind_location","actual":signature(&actual),"expected":signature(expected)}));
        }
    } else {
        same("output");
        same("output_by_alias");
        same("properties");
    }
    diffs
}
#[test]
fn all_original_1500_issue_kinds_are_preserved() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../ORIGINAL106-FROZEN-EXPECTATIONS.json");
    let frozen: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let runtime = RustSchemaRuntime::new().unwrap();
    let diffs = frozen["cases"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|c| compare(&runtime, c))
        .collect::<Vec<_>>();
    std::fs::write(
        path.parent()
            .unwrap()
            .join("SCHEMA1500-KIND-ACTUAL-DIFFS.json"),
        serde_json::to_vec_pretty(&diffs).unwrap(),
    )
    .unwrap();
    assert!(
        diffs.is_empty(),
        "{} divergences: {:?}",
        diffs.len(),
        diffs.iter().take(10).collect::<Vec<_>>()
    );
}
#[test]
fn all_original_106_constructor_construct_coercion_and_method_cases_match() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../ORIGINAL106-EXTENDED-FROZEN.json");
    let frozen: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let runtime = RustSchemaRuntime::new().unwrap();
    let mut diffs = vec![];
    let mut executed = 0;
    let mut explicit_native_unknown = 0;
    for group in ["cases", "construction_cases", "method_cases"] {
        for case in frozen[group].as_array().unwrap() {
            if case["native_nonfinite_profile"] == true {
                let failure = runtime
                    .validate_model(case["model"].as_str().unwrap(), &case["input"])
                    .unwrap_err();
                assert_eq!(failure.code, "schema_nonfinite_profile_unavailable");
                explicit_native_unknown += 1;
                continue;
            }
            executed += 1;
            diffs.extend(compare(&runtime, case));
        }
    }
    assert_eq!(executed, 8113);
    assert_eq!(explicit_native_unknown, 1);
    std::fs::write(
        path.parent().unwrap().join("SCHEMA8114-ACTUAL-DIFFS.json"),
        serde_json::to_vec_pretty(&diffs).unwrap(),
    )
    .unwrap();
    assert!(
        diffs.is_empty(),
        "{} divergences: {:?}",
        diffs.len(),
        diffs.iter().take(10).collect::<Vec<_>>()
    );
}
#[test]
fn original_issue_messages_remain_distinct_from_validation_status() {
    let evidence = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let runtime = RustSchemaRuntime::new().unwrap();
    let mut diffs = vec![];
    for file in [
        "ORIGINAL106-FROZEN-EXPECTATIONS.json",
        "ORIGINAL106-EXTENDED-FROZEN.json",
    ] {
        let frozen: Value =
            serde_json::from_slice(&std::fs::read(evidence.join(file)).unwrap()).unwrap();
        let groups = if file.contains("EXTENDED") {
            vec!["cases", "construction_cases", "method_cases"]
        } else {
            vec!["cases"]
        };
        for group in groups {
            for case in frozen[group].as_array().unwrap() {
                if case["original"]["status"] != "error"
                    || case["original"]["exception"] == "AttributeError"
                {
                    continue;
                }
                let actual = if case["operation"] == "constructor" {
                    runtime.validate_constructor(case["model"].as_str().unwrap(), &case["input"])
                } else {
                    runtime.validate_model(case["model"].as_str().unwrap(), &case["input"])
                }
                .unwrap();
                for (a, b) in actual["issues"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .zip(case["original"]["issues"].as_array().unwrap())
                {
                    if a["message"] != b["msg"] {
                        diffs.push(json!({"id":case["id"],"kind":b["type"],"actual":a["message"],"expected":b["msg"]}));
                    }
                }
            }
        }
    }
    std::fs::write(
        evidence.join("SCHEMA-ISSUE-MESSAGE-ACTUAL-DIFFS.json"),
        serde_json::to_vec_pretty(&diffs).unwrap(),
    )
    .unwrap();
    assert!(
        diffs.is_empty(),
        "{} message divergences: {:?}",
        diffs.len(),
        diffs.iter().take(10).collect::<Vec<_>>()
    );
}
#[test]
fn original_scalar_and_python_text_boundaries_are_preserved() {
    let evidence = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let frozen: Value = serde_json::from_slice(
        &std::fs::read(evidence.join("ORIGINAL106-SCALAR-BOUNDARIES-FROZEN.json")).unwrap(),
    )
    .unwrap();
    let runtime = RustSchemaRuntime::new().unwrap();
    let mut diffs = vec![];
    let mut unknown = 0;
    for case in frozen["cases"].as_array().unwrap() {
        if case["native_nonfinite_profile"] == true {
            unknown += 1;
            assert_eq!(
                runtime
                    .validate_model(case["model"].as_str().unwrap(), &case["input"])
                    .unwrap_err()
                    .code,
                "schema_nonfinite_profile_unavailable"
            );
            continue;
        }
        diffs.extend(compare(&runtime, case));
    }
    std::fs::write(
        evidence.join("SCALAR-BOUNDARIES-ACTUAL-DIFFS.json"),
        serde_json::to_vec_pretty(&diffs).unwrap(),
    )
    .unwrap();
    println!(
        "actual {} cases / {} retained native unknown",
        frozen["cases"].as_array().unwrap().len(),
        unknown
    );
    assert!(
        diffs.is_empty(),
        "{} scalar divergences: {:?}",
        diffs.len(),
        diffs.iter().take(10).collect::<Vec<_>>()
    );
}

#[test]
fn custom_python_float_and_plain_pydantic_numeric_boundaries_stay_distinct() {
    let evidence = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let runtime = RustSchemaRuntime::new().unwrap();
    let mut diffs = vec![];
    let mut unknown = 0;
    let mut executed = 0;
    for file in [
        "ORIGINAL106-UNICODE-NUMERIC-FROZEN.json",
        "ORIGINAL106-HUGE-NUMERIC-FROZEN.json",
    ] {
        let frozen: Value =
            serde_json::from_slice(&std::fs::read(evidence.join(file)).unwrap()).unwrap();
        for case in frozen["cases"].as_array().unwrap() {
            if case["native_nonfinite_profile"] == true {
                unknown += 1;
                assert_eq!(
                    runtime
                        .validate_model(case["model"].as_str().unwrap(), &case["input"])
                        .unwrap_err()
                        .code,
                    "schema_nonfinite_profile_unavailable"
                );
            } else if case["original"]["status"] == "raised" {
                assert_eq!(case["original"]["exception"], "OverflowError");
                match runtime.validate_model(case["model"].as_str().unwrap(), &case["input"]) {
                    Err(error) if error.code == "schema_python_float_overflow" => {},
                    result => diffs.push(serde_json::json!({"id":case["id"],"expected":"schema_python_float_overflow","actual":result})),
                }
                executed += 1;
            } else {
                diffs.extend(compare(&runtime, case));
                executed += 1;
            }
        }
    }
    std::fs::write(
        evidence.join("UNICODE-HUGE-NUMERIC-ACTUAL-DIFFS.json"),
        serde_json::to_vec_pretty(&diffs).unwrap(),
    )
    .unwrap();
    println!("actual {executed} finite/error cases / {unknown} retained native unknown");
    assert!(
        diffs.is_empty(),
        "{} numeric divergences: {:?}",
        diffs.len(),
        diffs.iter().take(8).collect::<Vec<_>>()
    );
}
