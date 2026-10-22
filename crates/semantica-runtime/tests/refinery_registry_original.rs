//! Consumes the immutable independent original thirty-case data oracle.
use semantica_runtime::sha256 as private_fixture_sha256;
#[path = "../../../build_support/private_test_fixtures.rs"]
mod private_test_fixtures;

#[test]
fn actual_original_thirty_registry_and_descriptor_controls() {
    use serde_json::Value;
    use std::collections::BTreeMap;
    let inputs: Value = serde_json::from_str(&private_test_fixtures::text(
        "planning-registry-overlay/ORIGINAL-INPUTS-01.json",
    ))
    .unwrap();
    let expected: Value = serde_json::from_str(&private_test_fixtures::text(
        "planning-registry-overlay/ORIGINAL-ORACLE-01.json",
    ))
    .unwrap();
    assert_eq!(inputs["cases"].as_array().unwrap().len(), 30);
    assert_eq!(
        expected["input_sha256"],
        "b14678815dca4c07492c5e7f0b5d32799c3395c387a2243010ddf29a3b5a6f1a"
    );
    let mut failures = Vec::new();
    for (input, original) in inputs["cases"]
        .as_array()
        .unwrap()
        .iter()
        .zip(expected["cases"].as_array().unwrap())
    {
        assert_eq!(input["case_id"], original["case_id"]);
        let events: Vec<_> = input["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| (e["filename"].as_str().unwrap().into(), e["value"].clone()))
            .collect();
        let records: BTreeMap<_, _> = input["records"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                (
                    (
                        r["package_id"].as_str().unwrap().into(),
                        r["version"].as_str().unwrap().into(),
                    ),
                    r["record"].clone(),
                )
            })
            .collect();
        let actual = semantica_runtime::refinery_registry_algorithms::reconstruct(
            input["workspace"]["registry_id"].as_str().unwrap(),
            &input["pointer"],
            &events,
            &records,
        );
        if actual.is_ok() != original["registry"]["accepted"].as_bool().unwrap() {
            failures.push(format!("{} registry {:?}", input["case_id"], actual));
        } else if let Ok(value) = actual {
            if value != original["registry"]["value"] {
                failures.push(format!("{} registry value", input["case_id"]));
            }
        }
        for (event, oracle) in events.iter().zip(original["events"].as_array().unwrap()) {
            let actual = semantica_runtime::refinery_registry_algorithms::registry_event(&event.1);
            if actual.is_ok() != oracle["accepted"].as_bool().unwrap() {
                failures.push(format!("{} event {:?}", input["case_id"], actual));
            }
        }
        for (record, oracle) in input["records"]
            .as_array()
            .unwrap()
            .iter()
            .zip(original["descriptors"].as_array().unwrap())
        {
            let actual =
                semantica_runtime::refinery_registry_algorithms::descriptor(&record["record"]);
            if actual.is_ok() != oracle["accepted"].as_bool().unwrap() {
                failures.push(format!("{} descriptor {:?}", input["case_id"], actual));
            } else if let Ok(value) = actual {
                if value != oracle["value"] {
                    failures.push(format!("{} descriptor value", input["case_id"]));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
