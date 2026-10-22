//! Explicit synthetic API names/scopes; never a native execution receipt.
#[path = "construction_fixture.rs"]
pub mod legacy;
use legacy::{ConstructionFixture, occurrence::definition, seal};
use oe_contracts::CadAssemblyConstructionRequest2;
use oe_core::{SkillRoot, sha256};
use oe_nx_protocol::{FileEntry, JobManifest, NativeResult, decode};
use serde_json::{Value, json};
use std::{fs, path::Path};

pub fn scope_fixture(root: Option<(SkillRoot, &Path)>) -> ConstructionFixture {
    let mut f = match root {
        Some((r, p)) => ConstructionFixture::in_root(r, p),
        None => ConstructionFixture::new(),
    };
    f.edit_creator(|directory, _, native| {
        for key in ["before_save_record", "after_save_record"] {
            let path = directory.join(native.readback[key]["path"].as_str().unwrap());
            let mut event: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            for o in event["observation"]["occurrences"].as_array_mut().unwrap() {
                o["reference_set"] = json!("None");
            }
            let raw = serde_json::to_vec(&event).unwrap();
            fs::write(path, &raw).unwrap();
            native.readback[key]["sha256"] = json!(sha256(raw));
        }
    });
    edit_inspector(&mut f, |params, native| {
        params["schema"] = json!("oev2.nx-assembly-reference-sets-params/2");
        params["capture_reference_sets"] = json!(true);
        for o in native.readback["assembly"]["occurrences"]
            .as_array_mut()
            .unwrap()
        {
            o["reference_set"] = json!("None");
        }
        let members=native.readback["assembly"]["occurrences"].as_array().unwrap().iter().map(|o| json!({
            "occurrence_id":o["occurrence_id"],"part_occurrence_tag":o["part_occurrence_tag"],
            "instance_tag":o["instance_tag"],"instance_reference_set":"None","part_occurrence_reference_set":"None"
        })).collect::<Vec<_>>();
        native.readback["reference_sets"] = json!({"schema":"oev2.nx-assembly-reference-sets/2",
            "api":"UF_ASSEM_ask_component_data","root_prototype_id":native.readback["assembly"]["root_prototype_id"],
            "complete":true,"members":members});
    });
    f
}
pub fn request(f: &ConstructionFixture) -> CadAssemblyConstructionRequest2 {
    CadAssemblyConstructionRequest2 {
        schema: "oev2.cad-assembly-construction/2".into(),
        project: f.request.project.clone(),
        assembly: f.request.assembly.clone(),
        creator_job: f.request.creator_job.clone(),
        creator_native_result_ref: f.request.creator_native_result_ref.clone(),
    }
}
pub fn edit_inspector(
    f: &mut ConstructionFixture,
    edit: impl FnOnce(&mut Value, &mut NativeResult),
) {
    let directory =
        f.f.root
            .path()
            .join(&f.request.assembly.job.path)
            .parent()
            .unwrap()
            .to_path_buf();
    let mut job: JobManifest = decode(&fs::read(directory.join("job.json")).unwrap()).unwrap();
    let mut native: NativeResult =
        decode(&fs::read(directory.join("native-result.json")).unwrap()).unwrap();
    let mut params: Value =
        serde_json::from_slice(&fs::read(directory.join("params.json")).unwrap()).unwrap();
    edit(&mut params, &mut native);
    let raw = serde_json::to_vec(&params).unwrap();
    fs::write(directory.join("params.json"), &raw).unwrap();
    job.params_schema = params["schema"].as_str().unwrap().into();
    for capability in &mut job.worker.capabilities {
        if capability.action == job.action {
            capability.params_schema = job.params_schema.clone();
        }
    }
    job.params = FileEntry::from_bytes("params.json", &raw).unwrap();
    seal(&directory, &job, &mut native);
    f.request.assembly.job.sha256 = sha256(fs::read(directory.join("job.json")).unwrap());
    let mut p = f.f.read_json(&f.request.project);
    p["evidence"]["assembly-job"] = json!(f.request.assembly.job.sha256);
    p["evidence"][&f.request.assembly.native_result_ref] = json!(sha256(
        fs::read(directory.join("native-result.json")).unwrap()
    ));
    f.request.project = f.f.write_json("construction2-project", &p);
    f.request.assembly.project_context = definition::context(&p, &f.request.assembly.object);
}
