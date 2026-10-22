#[path = "support/construction2_fixture.rs"]
mod fixture;
use oe_cad::evidence::correspondence::construction;
use serde_json::json;

#[test]
fn construction2_distinguishes_vendor_names_and_requires_actual_occurrences() {
    let f = fixture::scope_fixture(None);
    assert_eq!(
        construction::verify(&f.f.root, &f.request)
            .unwrap_err()
            .to_string(),
        "cad_construction_added_instance_reused_or_definition_differs"
    );
    let request = fixture::request(&f);
    let received = construction::verify2(&f.f.root, &request).unwrap();
    assert_eq!(received.request(), &request);
    assert_eq!(
        received.report()["schema"],
        "oev2.cad-assembly-construction-report/2"
    );
    assert_eq!(
        received.report()["reference_set_contract"]["inspection"]["members"][0]["part_occurrence_reference_set"],
        "None"
    );
    assert_eq!(
        received.report()["instances"][0]["creation"]["outcome"]["reference_set"],
        "Entire Part"
    );
    assert_eq!(received.report()["engineering_adoption"], "not_established");
    assert!(received.require_current_instance(&["A".into()]).is_ok());
    assert!(
        received
            .require_current_instance(&["absent".into()])
            .is_err()
    );
}

#[test]
fn construction2_rejects_missing_scope_proof_and_unrequested_capture() {
    for missing in [true, false] {
        let mut f = fixture::scope_fixture(None);
        fixture::edit_inspector(&mut f, |params, native| {
            if missing {
                native
                    .readback
                    .as_object_mut()
                    .unwrap()
                    .remove("reference_sets");
            } else {
                params["schema"] = json!(oe_nx_worker::assembly::PARAMS_SCHEMA);
                params
                    .as_object_mut()
                    .unwrap()
                    .remove("capture_reference_sets");
            }
        });
        let error = construction::verify2(&f.f.root, &fixture::request(&f))
            .unwrap_err()
            .to_string();
        assert!(
            error.contains(if missing {
                "cad_construction_reference_sets_missing"
            } else {
                "cad_construction_reference_set_capture_required"
            }),
            "{error}"
        );
    }
}

#[test]
fn construction2_rejects_wrong_scope_tags_coverage_and_non_entire() {
    for fault in [
        "occurrence",
        "instance",
        "definition_name",
        "actual_name",
        "duplicate",
        "missing",
        "api",
        "root",
        "partial",
    ] {
        let mut f = fixture::scope_fixture(None);
        fixture::edit_inspector(&mut f, |_, native| {
            let v = &mut native.readback["reference_sets"];
            match fault {
                "occurrence" => v["members"][0]["part_occurrence_tag"] = json!(9999),
                "instance" => v["members"][0]["instance_tag"] = json!(9999),
                "definition_name" => {
                    v["members"][0]["instance_reference_set"] = json!("Entire Part")
                }
                "actual_name" => v["members"][0]["part_occurrence_reference_set"] = json!("BODY"),
                "duplicate" => v["members"][1] = v["members"][0].clone(),
                "missing" => {
                    v["members"].as_array_mut().unwrap().pop();
                }
                "api" => v["api"] = json!("NXOpen"),
                "root" => v["root_prototype_id"] = json!("other.prt"),
                _ => v["complete"] = json!(false),
            }
        });
        let error = construction::verify2(&f.f.root, &fixture::request(&f))
            .unwrap_err()
            .to_string();
        let expected = match fault {
            "occurrence" | "instance" | "definition_name" => {
                "assembly_reference_sets_tags_or_definition_differs"
            }
            "actual_name" => "assembly_reference_sets_entire_part_required",
            "duplicate" => "assembly_reference_sets_duplicate_occurrence",
            "missing" => "assembly_reference_sets_coverage_differs",
            "api" => "assembly_reference_sets_contract_invalid",
            _ => "assembly_reference_sets_identity_or_completeness_differs",
        };
        assert!(error.contains(expected), "{fault}: {error}");
    }
}

#[test]
fn construction2_keeps_original_process_and_save_obligations() {
    for same_process in [true, false] {
        let mut f = fixture::scope_fixture(None);
        f.edit_creator(|_, _, native| {
            if same_process {
                native.process.pid = 42;
                native.process.creation_time_100ns = 2;
            } else {
                native.readback["save"]["file_error_count"] = json!(1);
            }
        });
        let error = construction::verify2(&f.f.root, &fixture::request(&f))
            .unwrap_err()
            .to_string();
        assert!(
            error.contains(if same_process {
                "cad_construction_independent_process_required"
            } else {
                "cad_construction_creation_or_save_incomplete"
            }),
            "{error}"
        );
    }
}
