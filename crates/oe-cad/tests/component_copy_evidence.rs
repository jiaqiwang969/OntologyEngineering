#[path = "support/copy_fixture.rs"]
mod fixture;
use oe_cad::evidence::correspondence::{component_copy, construction};
use serde_json::json;

#[test]
fn copy_receiving_binds_versions_and_independent_saved_reopen() {
    let f = fixture::CopyFixture::new();
    let c = component_copy::verify(&f.base.f.root, &f.request).unwrap();
    assert_eq!(c.request(), &f.request);
    assert_eq!(c.report()["independent_reinspection_checked"], true);
    assert_eq!(c.report()["engineering_adoption"], "not_established");
    assert_eq!(
        c.report()["copy"]["new_engineering_adoption"],
        "not_established"
    );
    assert!(c.require_current_copy(&["C".into()]).is_ok());
    assert!(c.require_current_copy(&["A".into()]).is_err());
    assert_ne!(
        c.report()["native_outcome"]["returned_component_tags"][0],
        c.report()["assembly"]["assembly"]["occurrences"][2]["part_occurrence_tag"]
    );
    let mut old = fixture::construction::request(&f.base);
    old.creator_job = f.request.copy_job.clone();
    old.assembly = f.request.assembly.clone();
    assert!(
        construction::verify2(&f.base.f.root, &old)
            .unwrap_err()
            .to_string()
            .contains("cad_construction_requires_source_assembly_creator")
    );
}

#[test]
fn copy_receiving_rejects_substitution_and_incomplete_results() {
    for (key, pointer, value, expected) in [
        (
            "call",
            "/observation/returned_count",
            json!(0),
            "copy_native_call_partial_or_failed",
        ),
        (
            "call",
            "/observation/complete",
            json!(false),
            "copy_native_call_partial_or_failed",
        ),
        (
            "call",
            "/observation/rename_code",
            json!(7),
            "copy_native_call_partial_or_failed",
        ),
        (
            "saved",
            "/observation/return_code",
            json!(7),
            "cad_copy_save_incomplete",
        ),
        (
            "saved",
            "/observation/file_error_count",
            json!(1),
            "cad_copy_save_incomplete",
        ),
        (
            "call",
            "/sequence",
            json!(100),
            "cad_copy_event_order_differs",
        ),
        (
            "before_reference_sets",
            "/observation/members/0/part_occurrence_reference_set",
            json!("BODY"),
            "copy_retained_reference_set_changed",
        ),
        (
            "saved_reference_sets",
            "/observation/complete",
            json!(false),
            "assembly_reference_sets_identity_or_completeness_differs",
        ),
    ] {
        let mut f = fixture::CopyFixture::new();
        f.edit_event(key, |event| *event.pointer_mut(pointer).unwrap() = value);
        let error = component_copy::verify(&f.base.f.root, &f.request)
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "{key} {pointer}: {error}");
    }
}

#[test]
fn copy_receiving_requires_new_version_and_matching_target_identity() {
    for fault in [
        "id",
        "version",
        "conditions",
        "same_version",
        "process",
        "root",
    ] {
        let mut f = fixture::CopyFixture::new();
        match fault {
            "id" => f.request.assembly.object.object_id = "other".into(),
            "version" => f.request.assembly.object.revision = "other".into(),
            "conditions" => f.request.assembly.object.condition_scope = "other".into(),
            "same_version" => {
                f.request.assembly.object.revision = f.request.before.object.revision.clone()
            }
            "root" => f.edit_copy(|directory, _, native| {
                let file = &mut native.artifacts[0];
                let raw = b"different saved root";
                std::fs::write(directory.join(file.path.as_str()), raw).unwrap();
                file.identity = oe_nx_protocol::BlobIdentity::of(raw);
            }),
            _ => f.edit_copy(|_, _, native| {
                native.process.pid = 42;
                native.process.creation_time_100ns = 2;
            }),
        }
        assert!(
            component_copy::verify(&f.base.f.root, &f.request).is_err(),
            "{fault}"
        );
    }
}

#[test]
fn copy_receiving_requires_original_bytes_and_both_raw_scope_captures() {
    for fault in ["source", "capture", "sets"] {
        let mut f = fixture::CopyFixture::new();
        if fault == "source" {
            // Re-seal the complete request and manifest with different original
            // source bytes. Integrity alone must not substitute the old version.
            f.edit_copy(|directory, job, native| {
                let params: serde_json::Value =
                    serde_json::from_slice(&std::fs::read(directory.join("params.json")).unwrap())
                        .unwrap();
                let name = format!("inputs/{}", params["intent"]["path"].as_str().unwrap());
                let mut intent: serde_json::Value =
                    serde_json::from_slice(&std::fs::read(directory.join(&name)).unwrap()).unwrap();
                let bytes = b"substituted original assembly";
                std::fs::write(directory.join("inputs/root.prt"), bytes).unwrap();
                let entry = job
                    .inputs
                    .iter_mut()
                    .find(|a| a.path.as_str() == "inputs/root.prt")
                    .unwrap();
                *entry = oe_nx_protocol::FileEntry::from_bytes("inputs/root.prt", bytes).unwrap();
                let source = intent["source"]["files"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|a| a["path"] == "root.prt")
                    .unwrap();
                source["bytes"] = json!(bytes.len());
                source["sha256"] = json!(oe_core::sha256(bytes));
                let total: u64 = intent["source"]["files"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|a| a["bytes"].as_u64().unwrap())
                    .sum();
                intent["source"]["max_total_bytes"] = json!(total);
                let raw = serde_json::to_vec(&intent).unwrap();
                std::fs::write(directory.join(&name), &raw).unwrap();
                *job.inputs
                    .iter_mut()
                    .find(|a| a.path.as_str() == name)
                    .unwrap() = oe_nx_protocol::FileEntry::from_bytes(&name, &raw).unwrap();
                let mut params = params;
                params["intent"]["sha256"] = json!(oe_core::sha256(&raw));
                native.readback["intent"] = params["intent"].clone();
                let raw = serde_json::to_vec(&params).unwrap();
                std::fs::write(directory.join("params.json"), &raw).unwrap();
                job.params = oe_nx_protocol::FileEntry::from_bytes("params.json", &raw).unwrap();
            });
        } else {
            let directory = &f.prior_directory;
            let mut job: oe_nx_protocol::JobManifest =
                oe_nx_protocol::decode(&std::fs::read(directory.join("job.json")).unwrap())
                    .unwrap();
            let mut native: oe_nx_protocol::NativeResult = oe_nx_protocol::decode(
                &std::fs::read(directory.join("native-result.json")).unwrap(),
            )
            .unwrap();
            if fault == "sets" {
                native
                    .readback
                    .as_object_mut()
                    .unwrap()
                    .remove("reference_sets");
            } else {
                let mut params: serde_json::Value =
                    serde_json::from_slice(&std::fs::read(directory.join("params.json")).unwrap())
                        .unwrap();
                params
                    .as_object_mut()
                    .unwrap()
                    .remove("capture_reference_sets");
                params["schema"] = json!(oe_nx_worker::assembly::PARAMS_SCHEMA);
                job.params_schema = oe_nx_worker::assembly::PARAMS_SCHEMA.into();
                for c in &mut job.worker.capabilities {
                    if c.action == job.action {
                        c.params_schema = job.params_schema.clone();
                    }
                }
                let raw = serde_json::to_vec(&params).unwrap();
                std::fs::write(directory.join("params.json"), &raw).unwrap();
                job.params = oe_nx_protocol::FileEntry::from_bytes("params.json", &raw).unwrap();
            }
            fixture::construction::legacy::seal(directory, &job, &mut native);
            f.request.before.job = fixture::reference(&f.base.f.root, &directory.join("job.json"));
        }
        let error = component_copy::verify(&f.base.f.root, &f.request)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains(match fault {
                "source" => "cad_copy_original_source_bytes_differ",
                "capture" => "cad_copy_reference_set_capture_required",
                _ => "cad_copy_inspected_reference_sets_missing",
            }),
            "{fault}: {error}"
        );
    }
}
