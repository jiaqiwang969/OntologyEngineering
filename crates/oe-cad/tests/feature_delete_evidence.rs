//! Software receiving controls. No NX process is launched by these tests.
#[path = "support/feature_delete_fixture.rs"]
mod fixture;
use oe_cad::evidence::correspondence::feature_delete;
use serde_json::json;
use std::fs;

#[test]
fn deletion_receiving_requires_actual_lineage_and_independent_reopen() {
    let f = fixture::DeleteFixture::new();
    let result = feature_delete::verify(&f.f.root, &f.request).unwrap();
    assert_eq!(result.request(), &f.request);
    assert_eq!(result.report()["deleted_feature_id"], "hole:left");
    assert_eq!(
        result.report()["current_target_mapping"],
        "missing_in_complete_reopened_geometry"
    );
    assert_eq!(result.report()["independent_reinspection_checked"], true);
    assert_eq!(result.report()["engineering_adoption"], "not_established");
    assert_eq!(
        result.correspondence().report()["changes"][0]["correspondence_status"],
        "declared_only"
    );
    assert_eq!(result.report()["native_outcome"]["list_item_tag"], 702);
    assert!(result.impact_seed_ids().contains("inspection:left"));
    assert!(result.impact_seed_ids().contains("deletion-native"));
    // An apparently complete saved result may not re-use its mutating process
    // as either the independent detailed or full-assembly observer.
    for (pid, birth, expected) in [
        (45, 45, "cad_delete_independent_detail_process_required"),
        (44, 44, "cad_delete_independent_assembly_process_required"),
    ] {
        let mut f = fixture::DeleteFixture::new();
        f.edit_delete(|_, _, native| {
            native.process.pid = pid;
            native.process.creation_time_100ns = birth;
        });
        let e = feature_delete::verify(&f.f.root, &f.request).unwrap_err();
        assert!(e.to_string().contains(expected), "{e:#}");
    }
}

#[test]
fn deletion_receiving_distinguishes_same_leaf_parts_by_complete_saved_sources() {
    let mut f = fixture::DeleteFixture::same_leaf_target();
    // Both root.prt and parts/root.prt have the same leaf. The target is not
    // the assembly root, and neither artifact order nor basename selects it.
    f.edit_delete(|_, _, native| native.artifacts.reverse());
    let result = feature_delete::verify(&f.f.root, &f.request).unwrap();
    let report = result.report();
    assert_eq!(
        report["assembly"]["assembly"]["root_prototype_id"],
        "root.prt"
    );
    assert_eq!(
        report["assembly"]["assembly"]["prototypes"][1]["prototype_id"],
        "parts/root.prt"
    );
    assert_eq!(report["deleted_feature_id"], "hole:left");
    assert_eq!(report["independent_reinspection_checked"], true);
    assert_eq!(report["engineering_adoption"], "not_established");
    assert_eq!(
        result.correspondence().report()["changes"][0]["correspondence_status"],
        "declared_only"
    );

    for (name, missing) in [
        ("out/build/root.prt", false),
        ("out/build/parts/root.prt", false),
        ("out/build/root.prt", true),
        ("out/build/parts/root.prt", true),
    ] {
        let mut f = fixture::DeleteFixture::same_leaf_target();
        f.edit_delete(|dir, _, native| {
            if missing {
                native.artifacts.retain(|file| file.path.as_str() != name);
            } else {
                // Reseal the actual replacement bytes; a stale outer hash is
                // not the reason this source substitution must be refused.
                let raw = b"Different synthetic same-leaf source, not a NX part";
                fs::write(dir.join(name), raw).unwrap();
                let file = native
                    .artifacts
                    .iter_mut()
                    .find(|file| file.path.as_str() == name)
                    .unwrap();
                file.identity = oe_nx_protocol::BlobIdentity::of(raw);
            }
        });
        assert!(
            feature_delete::verify(&f.f.root, &f.request).is_err(),
            "complete same-leaf closure required: {name}, missing={missing}"
        );
    }
}

#[test]
fn deletion_receiving_rejects_wrong_selection_partial_call_or_save() {
    for (key, pointer, value) in [
        ("selection", "/observation/feature_tag", json!(703)),
        (
            "feature_inventory",
            "/observation/features/1/owning_part_tag",
            json!(12),
        ),
        (
            "feature_inventory",
            "/observation/features/1/face_tags",
            json!([608]),
        ),
        (
            "feature_inventory",
            "/observation/features/1/query_codes/2",
            json!(7),
        ),
        ("feature_inventory", "/observation/complete", json!(false)),
        ("call", "/observation/list_item_tag", json!(703)),
        ("call", "/observation/list_count", json!(2)),
        ("call", "/observation/list_delete_code", json!(7)),
        ("call", "/observation/return_code", json!(7)),
        ("call", "/observation/update_code", json!(null)),
        ("call", "/observation/call_returned", json!(false)),
        ("call", "/observation/complete", json!(false)),
        ("call", "/sequence", json!(100)),
        ("saved", "/observation/return_code", json!(7)),
        ("saved", "/observation/file_error_count", json!(1)),
        (
            "saved_reference_sets",
            "/observation/complete",
            json!(false),
        ),
    ] {
        let mut f = fixture::DeleteFixture::new();
        f.edit_event(key, |v| *v.pointer_mut(pointer).unwrap() = value);
        assert!(
            feature_delete::verify(&f.f.root, &f.request).is_err(),
            "{key} {pointer}"
        );
    }
    let mut f = fixture::DeleteFixture::new();
    f.edit_event("feature_inventory", |v| {
        let mut duplicate = v["observation"]["features"][1].clone();
        duplicate["feature_tag"] = json!(799);
        v["observation"]["features"]
            .as_array_mut()
            .unwrap()
            .push(duplicate);
    });
    let error = feature_delete::verify(&f.f.root, &f.request).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("delete_feature_missing_or_ambiguous"),
        "{error:#}"
    );
}

#[test]
fn deletion_receiving_preserves_non_target_geometry_and_assembly() {
    for (key, pointer, value) in [
        (
            "after_detail",
            "/observation/model_edges/entries/0/arc/radius",
            json!(6.1),
        ),
        (
            "after_detail",
            "/observation/solid_face_inventory_complete",
            json!(false),
        ),
        (
            "after_detail",
            "/observation/solid_face_inventory/6/radius",
            json!(6.1),
        ),
        (
            "after_detail",
            "/observation/exact_work_csys_bounds/0/absolute_axis_bounds/3",
            json!(41.),
        ),
        (
            "after_detail",
            "/observation/exact_work_csys_bounds/0/min_corner/0",
            json!(-41.),
        ),
        (
            "saved_detail",
            "/observation/exact_work_csys_bounds/0/distances/0",
            json!(81.),
        ),
        (
            "saved_detail",
            "/observation/exact_work_csys_bounds/0/directions/0/0",
            json!(2.),
        ),
        (
            "saved_detail",
            "/observation/model_edges/complete",
            json!(false),
        ),
        (
            "after",
            "/observation/prototypes/1/solids/0/volume_m3",
            json!(0.0000256),
        ),
        (
            "after",
            "/observation/occurrences/0/name",
            json!("different-instance"),
        ),
        (
            "saved_inventory",
            "/observation/occurrences/1/prototype_id",
            json!("root.prt"),
        ),
        (
            "after_reference_sets",
            "/observation/members/0/part_occurrence_reference_set",
            json!("BODY"),
        ),
    ] {
        let mut f = fixture::DeleteFixture::new();
        f.edit_event(key, |v| *v.pointer_mut(pointer).unwrap() = value);
        assert!(
            feature_delete::verify(&f.f.root, &f.request).is_err(),
            "{key} {pointer}"
        );
    }
    for key in ["after_detail", "saved_detail"] {
        let mut f = fixture::DeleteFixture::new();
        f.edit_event(key, |v| v["observation"] = fixture::detail(false));
        let error = feature_delete::verify(&f.f.root, &f.request).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("delete_target_circle_not_proven_missing"),
            "{error:#}"
        );
    }
    for missing in ["min_corner", "directions", "distances"] {
        let mut f = fixture::DeleteFixture::new();
        f.edit_event("saved_detail", |v| {
            v["observation"]["exact_work_csys_bounds"][0]
                .as_object_mut()
                .unwrap()
                .remove(missing);
        });
        assert!(
            feature_delete::verify(&f.f.root, &f.request).is_err(),
            "missing raw native exact-bound field {missing}"
        );
    }
}

#[test]
fn deletion_receiving_binds_after_and_saved_detail_to_the_mutating_part_and_body() {
    for key in ["after_detail", "saved_detail"] {
        let mut f = fixture::DeleteFixture::new();
        f.edit_event(key, |v| v["observation"]["part_tag"] = json!(99));
        let error = feature_delete::verify(&f.f.root, &f.request).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("cad_delete_stage_detail_owner_differs"),
            "{key}: a different part must not certify the deleted target: {error:#}"
        );

        let mut f = fixture::DeleteFixture::new();
        f.edit_event(key, |v| {
            let d = &mut v["observation"];
            // Preserve complete, internally consistent geometry. Only its
            // live body identity differs from the delete call and assembly.
            d["solid_tags"] = json!([999]);
            for body in d["model_edges"]["bodies"].as_array_mut().unwrap() {
                body["body_tag"] = json!(999);
            }
            for edge in d["model_edges"]["entries"].as_array_mut().unwrap() {
                edge["body_tag"] = json!(999);
            }
            for face in d["solid_face_inventory"].as_array_mut().unwrap() {
                face["body_tag"] = json!(999);
            }
        });
        let error = feature_delete::verify(&f.f.root, &f.request).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("cad_delete_stage_detail_owner_differs"),
            "{key}: internally consistent geometry from another live body must reject: {error:#}"
        );
    }
}

#[test]
fn deletion_receiving_requires_the_saved_source_closure_and_original_declaration() {
    for fault in [
        "missing_saved_prototype",
        "substituted_saved_prototype",
        "no_call_event",
        "wrong_action",
        "wrong_change",
    ] {
        let mut f = fixture::DeleteFixture::new();
        if fault == "wrong_change" {
            f.request.change_id = "retained-right".into();
        } else {
            f.edit_delete(|dir, job, native| match fault {
                "missing_saved_prototype" => {
                    native.artifacts.pop();
                }
                "substituted_saved_prototype" => {
                    let e = &mut native.artifacts[1];
                    let raw = b"different synthetic saved plate";
                    fs::write(dir.join(e.path.as_str()), raw).unwrap();
                    e.identity = oe_nx_protocol::BlobIdentity::of(raw);
                }
                "no_call_event" => {
                    native.readback.as_object_mut().unwrap().remove("call");
                }
                "wrong_action" => {
                    job.action = "inspect_assembly".into();
                    job.worker.capabilities[0].action = job.action.clone();
                }
                _ => unreachable!(),
            });
        }
        assert!(
            feature_delete::verify(&f.f.root, &f.request).is_err(),
            "{fault}"
        );
    }
    // Correctly hashed new bytes may not silently replace a prior assembly input.
    let mut f = fixture::DeleteFixture::new();
    f.edit_delete(|dir, job, native| {
        let bytes = b"substitute original root";
        fs::write(dir.join("inputs/root.prt"), bytes).unwrap();
        *job.inputs
            .iter_mut()
            .find(|x| x.path.as_str() == "inputs/root.prt")
            .unwrap() = oe_nx_protocol::FileEntry::from_bytes("inputs/root.prt", bytes).unwrap();
        let params_path = dir.join("params.json");
        let mut params: serde_json::Value =
            serde_json::from_slice(&fs::read(&params_path).unwrap()).unwrap();
        let name = format!("inputs/{}", params["intent"]["path"].as_str().unwrap());
        let mut intent: serde_json::Value =
            serde_json::from_slice(&fs::read(dir.join(&name)).unwrap()).unwrap();
        let file = intent["source"]["files"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|x| x["path"] == "root.prt")
            .unwrap();
        file["bytes"] = json!(bytes.len());
        file["sha256"] = json!(oe_core::sha256(bytes));
        intent["source"]["max_total_bytes"] = json!(
            intent["source"]["files"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v["bytes"].as_u64().unwrap())
                .sum::<u64>()
        );
        let raw = serde_json::to_vec(&intent).unwrap();
        fs::write(dir.join(&name), &raw).unwrap();
        *job.inputs
            .iter_mut()
            .find(|x| x.path.as_str() == name)
            .unwrap() = oe_nx_protocol::FileEntry::from_bytes(&name, &raw).unwrap();
        params["intent"]["sha256"] = json!(oe_core::sha256(&raw));
        native.readback["intent"] = params["intent"].clone();
        let raw = serde_json::to_vec(&params).unwrap();
        fs::write(params_path, &raw).unwrap();
        job.params = oe_nx_protocol::FileEntry::from_bytes("params.json", &raw).unwrap();
    });
    let error = feature_delete::verify(&f.f.root, &f.request).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("cad_delete_original_source_differs"),
        "{error:#}"
    );
}
