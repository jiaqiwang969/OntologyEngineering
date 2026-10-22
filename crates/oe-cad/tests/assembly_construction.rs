//! Independent contract controls; these synthetic files never establish native execution.
#[path = "support/construction_fixture.rs"]
mod fixture;
use fixture::ConstructionFixture;
use oe_cad::evidence::correspondence::construction;
use oe_core::sha256;
use oe_nx_protocol::{BlobIdentity, FileEntry};
use serde_json::{Value, json};
use std::fs;

fn edit_event(f: &mut ConstructionFixture, key: &str, change: impl FnOnce(&mut Value)) {
    f.edit_creator(|directory, _, native| {
        let path = directory.join(native.readback[key]["path"].as_str().unwrap());
        let mut v: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        change(&mut v);
        let raw = serde_json::to_vec(&v).unwrap();
        fs::write(path, &raw).unwrap();
        native.readback[key]["sha256"] = json!(sha256(raw));
    });
}
fn edit_add(f: &mut ConstructionFixture, change: impl FnOnce(&mut Value)) {
    f.edit_creator(|directory, _, native| {
        let reference = &mut native.readback["load_status_records"][0];
        let path = directory.join(reference["record"]["path"].as_str().unwrap());
        let mut v: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let before_count = v["observation"]["observation_bytes"].clone();
        change(&mut v);
        if v["observation"]["observation_bytes"] == before_count {
            v["observation"]["observation_bytes"] = json!(
                v["observation"]["observations"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| serde_json::to_vec(x).unwrap().len())
                    .sum::<usize>()
            );
        }
        let raw = serde_json::to_vec(&v).unwrap();
        fs::write(path, &raw).unwrap();
        reference["record"]["identity"] = serde_json::to_value(BlobIdentity::of(&raw)).unwrap();
    });
}
#[test]
fn assembly_construction_binds_creator_save_and_independent_reopen() {
    let f = ConstructionFixture::new();
    let c = construction::verify(&f.f.root, &f.request).unwrap();
    assert_eq!(
        c.report()["status"],
        "bounded_creation_and_independent_reinspection"
    );
    assert_eq!(c.report()["instances"].as_array().unwrap().len(), 2);
    assert_eq!(c.report()["independent_reinspection_checked"], true);
    assert_eq!(c.report()["engineering_adoption"], "not_established");
    assert_eq!(c.report()["copy_delete_lineage"], "not_established");
    assert!(c.require_current_instance(&["A".into()]).is_ok());
    assert!(c.require_current_instance(&["B".into()]).is_ok());
    assert!(c.require_current_instance(&["missing".into()]).is_err());
    assert_ne!(
        c.report()["creator_process"],
        c.report()["inspection_process"]
    );
    // Creation tags were deliberately different; the file/source/path contract
    // remains necessary rather than relying on equal tags across processes.
    let current = &c.report()["assembly"]["assembly"]["occurrences"][0];
    assert_ne!(
        c.report()["instances"][0]["creation"]["outcome"]["component_tag"],
        current["part_occurrence_tag"]
    );
}
#[test]
fn assembly_construction_rejects_context_or_process_substitution() {
    for field in ["object_id", "revision", "condition_scope"] {
        let mut f = ConstructionFixture::new();
        match field {
            "object_id" => f.request.assembly.object.object_id = "other".into(),
            "revision" => f.request.assembly.object.revision = "other".into(),
            _ => f.request.assembly.object.condition_scope = "other".into(),
        }
        assert!(
            construction::verify(&f.f.root, &f.request)
                .unwrap_err()
                .to_string()
                .contains("cad_construction_object_revision_or_conditions_differ")
        );
    }
    let mut f = ConstructionFixture::new();
    f.edit_creator(|_, _, native| {
        native.process.pid = 42;
        native.process.creation_time_100ns = 2;
    });
    assert!(
        construction::verify(&f.f.root, &f.request)
            .unwrap_err()
            .to_string()
            .contains("cad_construction_independent_process_required")
    );
}
#[test]
fn assembly_construction_rejects_incomplete_add_or_save() {
    for kind in [
        "missing",
        "duplicate",
        "partial",
        "parent",
        "prototype",
        "instance",
        "event_identity",
        "phase",
        "reference_set",
    ] {
        let mut f = ConstructionFixture::new();
        edit_add(&mut f, |v| {
            if kind == "event_identity" {
                v["job_id"] = json!("other");
                return;
            }
            if kind == "phase" {
                v["phase"] = json!("different_phase");
                return;
            }
            let a = v["observation"]["observations"].as_array_mut().unwrap();
            match kind {
                "missing" => {
                    a.pop();
                }
                "duplicate" => a.push(a[0].clone()),
                "partial" => a[0]["outcome"]["complete"] = json!(false),
                "parent" => a[0]["outcome"]["definition_parent_tag"] = json!(9999),
                "prototype" => a[0]["outcome"]["prototype_tag"] = json!(9999),
                "instance" => a[0]["outcome"]["definition_instance_tag"] = json!(9999),
                _ => a[0]["outcome"]["reference_set"] = json!("None"),
            }
        });
        assert!(
            construction::verify(&f.f.root, &f.request).is_err(),
            "{kind}"
        );
    }
    let mut f = ConstructionFixture::new();
    f.edit_creator(|_, _, n| n.readback["save"]["file_error_count"] = json!(1));
    assert!(
        construction::verify(&f.f.root, &f.request)
            .unwrap_err()
            .to_string()
            .contains("cad_construction_creation_or_save_incomplete")
    );
    let mut f = ConstructionFixture::new();
    edit_event(&mut f, "after_save_record", |v| {
        v["phase"] = json!("source_assembly_before_save")
    });
    assert!(construction::verify(&f.f.root, &f.request).is_err());
}
#[test]
fn assembly_construction_rejects_wrong_output_or_inventory() {
    for name in ["root.prt", "plate.prt"] {
        let mut f = ConstructionFixture::new();
        f.edit_creator(|directory, _, native| {
            let path = format!("out/build/{name}");
            let raw = b"wrong saved file, with an internally valid new digest";
            fs::write(directory.join(&path), raw).unwrap();
            *native
                .artifacts
                .iter_mut()
                .find(|a| a.path.as_str() == path)
                .unwrap() = FileEntry::from_bytes(&path, raw).unwrap();
        });
        assert!(
            construction::verify(&f.f.root, &f.request)
                .unwrap_err()
                .to_string()
                .contains("cad_construction_saved_")
        );
    }
    let mut f = ConstructionFixture::new();
    edit_event(&mut f, "after_save_record", |v| {
        v["observation"]["occurrences"]
            .as_array_mut()
            .unwrap()
            .pop();
    });
    assert!(construction::verify(&f.f.root, &f.request).is_err());
    let mut f = ConstructionFixture::new();
    edit_event(&mut f, "after_save_record", |v| {
        v["observation"]["prototypes"][1]["native_units"] = json!("inch")
    });
    assert!(construction::verify(&f.f.root, &f.request).is_err());
}

#[test]
fn assembly_construction_checks_native_emission_order_and_byte_accounting() {
    let mut accepted = Vec::new();
    for kind in ["reversed-add-order", "wrong-byte-account"] {
        let mut f = ConstructionFixture::new();
        edit_add(&mut f, |v| {
            if kind == "reversed-add-order" {
                v["observation"]["observations"]
                    .as_array_mut()
                    .unwrap()
                    .reverse();
            } else {
                v["observation"]["observation_bytes"] =
                    json!(v["observation"]["observation_bytes"].as_u64().unwrap() + 1);
            }
        });
        accepted.push(construction::verify(&f.f.root, &f.request).is_ok());
    }
    assert_eq!(
        accepted,
        vec![false, false],
        "Original native emission order and exact byte account are required independently of valid endpoints"
    );
}
