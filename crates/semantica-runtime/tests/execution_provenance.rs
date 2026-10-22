use semantica_runtime::sha256 as private_fixture_sha256;
#[path = "../../../build_support/private_test_fixtures.rs"]
mod private_test_fixtures;

use semantica_runtime::execution_provenance as p;
use serde_json::{Value, json};

fn original() -> (Value, Value) {
    let input_fixture: &str =
        &private_test_fixtures::text("original/ORIGINAL-PURE-PROVENANCE-INPUTS.json");
    let oracle_fixture: &str =
        &private_test_fixtures::text("original/ORIGINAL-PURE-PROVENANCE-ORACLE.json");
    assert_eq!(
        semantica_runtime::sha256(input_fixture),
        "e68a78306a410b96dcdfc0ede96c6c92658833d2b5537dfdd4d357a77f20e8eb"
    );
    assert_eq!(
        semantica_runtime::sha256(oracle_fixture),
        "21e6d54b9b8be327eee4f53bcd33a29f15681753b8c96f0a491055a09ee45c5c"
    );
    (
        semantica_contracts::json::strict_json(input_fixture.as_bytes()).unwrap(),
        semantica_contracts::json::strict_json(oracle_fixture.as_bytes()).unwrap(),
    )
}
fn built() -> Value {
    let (input, _) = original();
    let mut raw = json!({"created_at":input["time"],"runtime_version":input["runtime_version"],"runtime_commit":input["runtime_commit"],"runtime_artifact_sha256":input["runtime_artifact_sha256"],"package_id":input["package_id"],"package_version":input["package_version"],"package_digest":input["package_digest"],"asset_hashes":input["asset_hashes"],"chapter_contract_sha256":input["chapter_contract_sha256"],"dataset_sha256":semantica_runtime::sha256(input["dataset"]["canonical_nquads"].as_str().unwrap()),"dataset_quad_count":input["dataset"]["quad_count"],"dataset_revision":input["dataset"]["revision"],"output_hashes":input["output_hashes"]});
    for report in input["reports"].as_array().unwrap() {
        let kind = report["kind"].as_str().unwrap();
        raw[format!("{kind}_report")] = serde_json::to_value(
            p::execution_report(kind, &report["payload"], report["status"].as_str().unwrap())
                .unwrap(),
        )
        .unwrap();
    }
    serde_json::to_value(p::build_execution_receipt(&raw).unwrap()).unwrap()
}

#[test]
fn original_execution_receipt_exact() {
    let (_, oracle) = original();
    let actual = built();
    assert_eq!(actual, oracle["receipt"]);
    assert_eq!(
        p::canonical_json(&actual).unwrap(),
        p::canonical_json(&oracle["receipt"]).unwrap()
    );
    assert_eq!(
        p::receipt_bindings(&actual).unwrap(),
        oracle["receipt_bindings"]
    );
    assert!(p::verify_receipt(&actual).unwrap());
    assert!(p::verify_receipt_bindings(&actual).unwrap());
    assert_eq!(
        serde_json::to_value(p::decode_receipt(&p::canonical_json(&actual).unwrap()).unwrap())
            .unwrap(),
        actual
    );
}

#[test]
fn original_bundle_and_28_counterexamples() {
    let (_, oracle) = original();
    let receipt = built();
    let bundle = receipt["provenance_bundle"].clone();
    let independent = serde_json::to_value(
        p::build_provenance_bundle(&oracle["receipt_bindings"], Some("2026-08-19T00:00:00Z"))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(independent, oracle["bundle"]);
    let mut checked = 0;
    for case in oracle["cases"].as_array().unwrap() {
        let id = case["case_id"].as_str().unwrap();
        match id {
            "original-build-full-nine-record-bundle-and-receipt" => {
                assert_eq!(
                    json!(p::verify_bundle(&bundle).unwrap()),
                    case["bundle_integral"]
                );
                assert_eq!(
                    json!(p::verify_receipt(&receipt).unwrap()),
                    case["receipt_integral"]
                );
                assert_eq!(
                    json!(bundle["records"].as_array().unwrap().len()),
                    case["record_count"]
                );
                assert_eq!(bundle["bundle_sha256"], case["bundle_sha256"]);
                assert_eq!(receipt["receipt_sha256"], case["receipt_sha256"]);
            }
            id if id.starts_with("original-record-field-") => {
                let entry = &case["input_record"];
                let raw = String::from_utf8(p::canonical_json(entry).unwrap()).unwrap();
                assert_eq!(
                    p::validate_record(&raw, entry["checksum"].as_str().unwrap()).unwrap(),
                    *entry
                );
                assert_eq!(
                    json!(p::verify_checksum(entry, None).unwrap()),
                    case["record_integral"],
                    "{id}"
                );
                let mut changed = bundle.clone();
                changed["records"][0] = entry.clone();
                assert_eq!(
                    json!(p::verify_bundle(&changed).unwrap()),
                    case["bundle_integral_without_rehash"],
                    "{id}"
                );
            }
            "original-bundle-deleted-first-record"
            | "original-bundle-deleted-middle-record"
            | "original-bundle-reordered-records" => {
                let mut changed = bundle.clone();
                let records = changed["records"].as_array_mut().unwrap();
                if id.ends_with("deleted-first-record") {
                    records.remove(0);
                } else if id.ends_with("deleted-middle-record") {
                    records.remove(4);
                } else {
                    records.swap(0, 1);
                }
                assert_eq!(
                    json!(p::verify_bundle(&changed).unwrap()),
                    case["bundle_integral"],
                    "{id}"
                );
            }
            "original-bundle-bindings-tamper" => {
                let mut changed = bundle.clone();
                changed["bindings"]["asset_hashes"]["case-positive"] = json!("0".repeat(64));
                assert_eq!(
                    json!(p::verify_bundle(&changed).unwrap()),
                    case["bundle_integral"]
                );
            }
            "original-missing-oracle-report" => {
                let report =
                    serde_json::to_value(p::missing_execution_report("oracle").unwrap()).unwrap();
                assert_eq!(report, case["report"]);
                assert_eq!(
                    json!(p::verify_report(&report).unwrap()),
                    case["report_integral"]
                );
            }
            "original-receipt-runtime-fields-change" => {
                let mut changed = receipt.clone();
                for field in [
                    "runtime_commit",
                    "runtime_version",
                    "runtime_artifact_sha256",
                ] {
                    changed[field] = json!("");
                }
                assert_eq!(
                    json!(p::verify_receipt(&changed).unwrap()),
                    case["receipt_integral"]
                );
            }
            "original-receipt-report-tamper" => {
                let mut changed = receipt.clone();
                changed["cq_report"]["payload"] = json!({"synthetic":"changed"});
                assert_eq!(
                    json!(p::verify_report(&changed["cq_report"]).unwrap()),
                    case["report_integral"]
                );
                assert_eq!(
                    json!(p::verify_receipt(&changed).unwrap()),
                    case["receipt_integral"]
                );
            }
            _ => panic!("unhandled independently frozen original case: {id}"),
        }
        checked += 1;
    }
    assert_eq!(checked, 28);
    assert_eq!(json!(checked), oracle["case_count"]);
}

#[test]
fn original_checksums_and_defaults() {
    let (_, oracle) = original();
    for record in oracle["bundle"]["records"].as_array().unwrap() {
        assert_eq!(
            p::compute_checksum(record).unwrap(),
            record["checksum"].as_str().unwrap()
        );
        let entry = p::entry_from_dict(record, Some("fixed-unused-default")).unwrap();
        assert_eq!(
            p::compute_entry_checksum(&entry).unwrap(),
            record["checksum"].as_str().unwrap()
        );
        assert_eq!(
            serde_json::to_value(entry)
                .unwrap()
                .as_object()
                .unwrap()
                .len(),
            39
        );
    }
}

#[test]
fn constructor_rejections_are_not_content_passes() {
    let receipt = built();
    let mut bundle = receipt["provenance_bundle"].clone();
    bundle["records"] = json!([]);
    let mut content = bundle.clone();
    content.as_object_mut().unwrap().remove("bundle_sha256");
    bundle["bundle_sha256"] = json!(semantica_runtime::sha256(
        p::canonical_json(&content).unwrap()
    ));
    assert!(p::verify_bundle(&bundle).is_err());
    let mut bundle = receipt["provenance_bundle"].clone();
    bundle["bundle_id"] = json!(" ");
    assert!(p::verify_bundle(&bundle).is_err());
    let mut report = receipt["cq_report"].clone();
    report["status"] = json!("true");
    assert!(p::verify_report(&report).is_err());
    report["status"] = json!("passed");
    report["kind"] = json!("");
    assert!(p::verify_report(&report).is_err());
    let mut wrong = receipt.clone();
    wrong["schema_version"] = json!("2");
    assert!(p::verify_receipt(&wrong).is_err());
    assert!(p::decode_receipt(b"{\"asset_hashes\":{\"same\":1,\"same\":2}}").is_err());
    let record = &receipt["provenance_bundle"]["records"][0];
    let spaced = serde_json::to_string_pretty(record).unwrap();
    assert!(p::validate_record(&spaced, record["checksum"].as_str().unwrap()).is_err());
}

#[test]
fn original_47_json_defaults_and_checksum_boundaries() {
    let raw_input_fixture: &str =
        &private_test_fixtures::text("original/ORIGINAL-PROVENANCE-BOUNDARY-INPUTS.json");
    let raw_oracle_fixture: &str =
        &private_test_fixtures::text("original/ORIGINAL-PROVENANCE-BOUNDARY-ORACLE.json");
    assert_eq!(
        semantica_runtime::sha256(raw_input_fixture),
        "40444b9beb2c7b2a3bb7b9a817102d16ef723c3e73f61077bc72580ebf2c253d"
    );
    assert_eq!(
        semantica_runtime::sha256(raw_oracle_fixture),
        "c042f47bfe0d7a79df6d39e0fe499a90bfaf891d81f45d50a0dd6ff3aff4c6a9"
    );
    let input = semantica_contracts::json::strict_json(raw_input_fixture.as_bytes()).unwrap();
    let oracle = semantica_contracts::json::strict_json(raw_oracle_fixture.as_bytes()).unwrap();
    let plans = input["plans"].as_array().unwrap();
    let cases = oracle["cases"].as_array().unwrap();
    assert_eq!(plans.len(), 47);
    assert_eq!(cases.len(), plans.len());
    for (plan, expected) in plans.iter().zip(cases) {
        assert_eq!(plan["case_id"], expected["case_id"]);
        let id = plan["case_id"].as_str().unwrap();
        match plan["operation"].as_str().unwrap() {
            "entry_defaults" => {
                let entry =
                    p::entry_from_dict(&plan["entry"], Some(input["time"].as_str().unwrap()))
                        .unwrap();
                let wire = serde_json::to_value(&entry).unwrap();
                assert_eq!(wire, expected["entry"], "{id}");
                assert_eq!(wire.as_object().unwrap().len(), 39);
                assert_eq!(
                    p::compute_entry_checksum(&entry).unwrap(),
                    expected["object_checksum"].as_str().unwrap(),
                    "{id}"
                );
                assert_eq!(
                    p::compute_checksum(&wire).unwrap(),
                    expected["dict_checksum"].as_str().unwrap(),
                    "{id}"
                );
            }
            "checksum_dict" => {
                let result = p::compute_checksum(&plan["entry"]);
                if expected["outcome"] == "raised" {
                    assert!(result.is_err(), "{id}");
                } else {
                    assert_eq!(
                        result.unwrap(),
                        expected["checksum"].as_str().unwrap(),
                        "{id}"
                    );
                }
            }
            "checksum_sorted_repr" => assert_eq!(
                p::compute_dict_checksum(&plan["dict"]).unwrap(),
                expected["checksum"].as_str().unwrap(),
                "{id}"
            ),
            "bundle_sequence" => {
                let mut changed = built()["provenance_bundle"].clone();
                changed["records"][0]["sequence_id"] = plan["first_sequence"].clone();
                let mut content = changed.clone();
                content.as_object_mut().unwrap().remove("bundle_sha256");
                changed["bundle_sha256"] = json!(semantica_runtime::sha256(
                    p::canonical_json(&content).unwrap()
                ));
                assert_eq!(changed, expected["bundle"], "{id}");
                assert_eq!(
                    json!(p::verify_checksum(&changed["records"][0], None).unwrap()),
                    expected["record_integral"],
                    "{id}"
                );
                assert_eq!(
                    json!(p::verify_bundle(&changed).unwrap()),
                    expected["bundle_integral"],
                    "{id}"
                );
            }
            _ => panic!("unhandled original boundary {id}"),
        }
    }
}

#[test]
fn original_python_control_whitespace_and_numeric_time() {
    assert!(p::execution_report("\u{1c}\u{1d}\u{1e}\u{1f}", &json!({}), "passed").is_err());
    let mut bundle = built()["provenance_bundle"].clone();
    bundle["bundle_id"] = json!("\u{1c}\u{1d}\u{1e}\u{1f}");
    assert!(p::verify_bundle(&bundle).is_err());
    let mut receipt = built();
    receipt["created_at"] = json!(123);
    let full = serde_json::to_value(p::build_execution_receipt(&receipt).unwrap()).unwrap();
    assert_eq!(full["created_at"], json!(123));
    assert_eq!(full["provenance_bundle"]["generated_at"], json!(123));
}

#[test]
fn original_python_numeric_time_preserves_json_type() {
    let mut input = built();
    input["created_at"] = json!(123);
    let full = serde_json::to_value(p::build_execution_receipt(&input).unwrap()).unwrap();
    assert_eq!(full["created_at"], json!(123));
    assert_eq!(full["provenance_bundle"]["generated_at"], json!(123));
}

#[test]
fn original_19_constructor_boundaries_and_raw_timestamp() {
    let raw_input_fixture: &str =
        &private_test_fixtures::text("original/ORIGINAL-CONSTRUCTOR-BOUNDARY-INPUTS.json");
    let raw_oracle_fixture: &str =
        &private_test_fixtures::text("original/ORIGINAL-CONSTRUCTOR-BOUNDARY-ORACLE.json");
    assert_eq!(
        semantica_runtime::sha256(raw_input_fixture),
        "94a1094140458073d994862eb8bc52fe77e86d93cd2f5f512c234133d642f4de"
    );
    assert_eq!(
        semantica_runtime::sha256(raw_oracle_fixture),
        "9ab2b500fefdaf36861d56107b290fc41d0c1cdc8f67b235b505ad77445a8a83"
    );
    let input = semantica_contracts::json::strict_json(raw_input_fixture.as_bytes()).unwrap();
    let oracle = semantica_contracts::json::strict_json(raw_oracle_fixture.as_bytes()).unwrap();
    let plans = input["plans"].as_array().unwrap();
    let cases = oracle["cases"].as_array().unwrap();
    assert_eq!(plans.len(), 19);
    assert_eq!(cases.len(), 19);
    for (plan, expected) in plans.iter().zip(cases) {
        let id = plan["case_id"].as_str().unwrap();
        assert_eq!(plan["case_id"], expected["case_id"]);
        match plan["operation"].as_str().unwrap() {
            "report-kind" => {
                let result =
                    p::execution_report(plan["value"].as_str().unwrap(), &json!({}), "passed");
                if expected["outcome"] == "raised" {
                    assert!(result.is_err(), "{id}");
                } else {
                    let report = serde_json::to_value(result.unwrap()).unwrap();
                    assert_eq!(report, expected["report"], "{id}");
                    assert_eq!(
                        json!(p::verify_report(&report).unwrap()),
                        expected["report_integral"]
                    );
                }
            }
            "bundle-id" => {
                let mut bundle = built()["provenance_bundle"].clone();
                bundle["bundle_id"] = plan["value"].clone();
                let result = p::verify_bundle(&bundle);
                if expected["outcome"] == "raised" {
                    assert!(result.is_err(), "{id}");
                } else {
                    assert_eq!(bundle["bundle_id"], plan["value"]);
                    assert_eq!(
                        json!(result.unwrap()),
                        expected["bundle_integral_without_rehash"],
                        "{id}"
                    );
                }
            }
            "receipt-created-at" => {
                let mut input = built();
                input["created_at"] = plan["value"].clone();
                let full =
                    serde_json::to_value(p::build_execution_receipt(&input).unwrap()).unwrap();
                assert_eq!(full["created_at"], expected["receipt_created_at"], "{id}");
                assert_eq!(
                    json!(p::verify_receipt(&full).unwrap()),
                    expected["receipt_integral"],
                    "{id}"
                );
                let clock = &full["provenance_bundle"]["generated_at"];
                assert!(
                    full["provenance_bundle"]["records"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .all(|r| &r["timestamp"] == clock)
                );
                if plan["fixed_clock"] == true {
                    assert_eq!(full, expected["receipt"], "{id}");
                } else {
                    assert!(clock.as_str().is_some_and(|s| !s.is_empty()));
                    assert_eq!(
                        expected["dynamic_clock_identity_qualification"],
                        "not_qualified"
                    );
                }
            }
            _ => panic!("unhandled original constructor {id}"),
        }
    }
}
