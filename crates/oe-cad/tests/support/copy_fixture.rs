//! Independent synthetic protocol inputs; no native CAD execution is claimed.
#[path = "construction2_fixture.rs"]
pub mod construction;
use construction::legacy::{ConstructionFixture, occurrence::definition, seal};
use oe_contracts::{ArtifactReference, CadAssemblyCopyRequest1};
use oe_core::{SkillRoot, sha256};
use oe_nx_protocol::*;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub struct CopyFixture {
    pub base: ConstructionFixture,
    pub request: CadAssemblyCopyRequest1,
    pub prior_directory: PathBuf,
    pub current_directory: PathBuf,
}
fn copy_directory(a: &Path, b: &Path) {
    fs::create_dir(b).unwrap();
    for e in fs::read_dir(a).unwrap() {
        let e = e.unwrap();
        let target = b.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_directory(&e.path(), &target);
        } else {
            fs::copy(e.path(), target).unwrap();
        }
    }
}
pub fn reference(root: &SkillRoot, path: &Path) -> ArtifactReference {
    ArtifactReference {
        path: path
            .strip_prefix(root.path())
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/"),
        sha256: sha256(fs::read(path).unwrap()),
    }
}
pub fn record(
    directory: &Path,
    job: &str,
    sequence: u64,
    phase: &str,
    v: Value,
) -> ArtifactReference {
    let path = format!("receipts/{sequence:04}-{phase}.json");
    let raw = serde_json::to_vec(
        &json!({"schema":"cad-agent.nx-native-event/v2","job_id":job,
        "sequence":sequence,"phase":phase,"observation":v}),
    )
    .unwrap();
    fs::create_dir_all(directory.join("receipts")).unwrap();
    fs::write(directory.join(&path), &raw).unwrap();
    ArtifactReference {
        path,
        sha256: sha256(raw),
    }
}
fn sets(v: &Value) -> Value {
    json!({"schema":"oev2.nx-assembly-reference-sets/2","api":"UF_ASSEM_ask_component_data",
        "root_prototype_id":v["root_prototype_id"],"complete":true,
        "members":v["occurrences"].as_array().unwrap().iter().map(|o|json!({
            "occurrence_id":o["occurrence_id"],"part_occurrence_tag":o["part_occurrence_tag"],
            "instance_tag":o["instance_tag"],"instance_reference_set":"None","part_occurrence_reference_set":"None"})).collect::<Vec<_>>()})
}
impl CopyFixture {
    pub fn new() -> Self {
        Self::build(construction::scope_fixture(None))
    }
    pub fn in_root(root: SkillRoot, directory: &Path) -> Self {
        Self::build(construction::scope_fixture(Some((root, directory))))
    }
    fn build(base: ConstructionFixture) -> Self {
        let root = &base.f.root;
        let current_directory = root
            .path()
            .join(&base.request.assembly.job.path)
            .parent()
            .unwrap()
            .to_path_buf();
        let prior_directory = current_directory
            .parent()
            .unwrap()
            .join("prior-copy-inspection");
        copy_directory(&current_directory, &prior_directory);
        let mut prior_job: JobManifest =
            decode(&fs::read(prior_directory.join("job.json")).unwrap()).unwrap();
        let mut prior_native: NativeResult =
            decode(&fs::read(prior_directory.join("native-result.json")).unwrap()).unwrap();
        prior_job.job_id = JobId::new("synthetic-prior-copy-inspection").unwrap();
        prior_native.process.pid = 40;
        prior_native.process.creation_time_100ns = 1;
        seal(&prior_directory, &prior_job, &mut prior_native);
        let before_graph = prior_native.readback["assembly"].clone();
        let mut before = base.request.assembly.clone();
        before.object.revision = "before-copy".into();
        before.job = reference(root, &prior_directory.join("job.json"));
        before.native_result_ref = "prior-copy-inspection-native".into();
        let mut prior_project = base.f.read_json(&base.request.project);
        prior_project["revision"] = json!("before-copy");
        prior_project["objects"][&before.object.object_id] = json!(before.object.revision);
        prior_project["evidence"]["prior-copy-job"] = json!(before.job.sha256);
        prior_project["evidence"][&before.native_result_ref] =
            json!(reference(root, &prior_directory.join("native-result.json")).sha256);
        prior_project["decisions"] = json!(["drawing","fit","manufacturing","inspection"].map(|id|json!({
            "id":id,"object_id":before.object.object_id,"object_revision":before.object.revision,
            "condition_scope":before.object.condition_scope,"source_sha256":before.object.source_sha256,
            "conclusion":"unknown","adopted":false,"basis":["assembly-model"]})));
        for id in ["drawing", "fit", "manufacturing", "inspection"] {
            prior_project["dependencies"][id] = json!([before.object.object_id]);
        }
        before.project_context = definition::context(&prior_project, &before.object);
        let previous_project = base.f.write_json("before-copy-project", &prior_project);
        let mut current_job: JobManifest =
            decode(&fs::read(current_directory.join("job.json")).unwrap()).unwrap();
        let mut current_native: NativeResult =
            decode(&fs::read(current_directory.join("native-result.json")).unwrap()).unwrap();
        let mut after_graph = before_graph.clone();
        let mut new = after_graph["occurrences"][0].clone();
        new["occurrence_id"] = json!("root/000002");
        new["name"] = json!("C");
        new["part_occurrence_tag"] = json!(9001);
        new["instance_tag"] = json!(9002);
        after_graph["occurrences"].as_array_mut().unwrap().push(new);
        current_native.readback["assembly"] = after_graph.clone();
        current_native.readback["reference_sets"] = sets(&after_graph);
        let new_root = b"independent synthetic saved copied root";
        fs::write(current_directory.join("inputs/root.prt"), new_root).unwrap();
        *current_job
            .inputs
            .iter_mut()
            .find(|f| f.path.as_str() == "inputs/root.prt")
            .unwrap() = FileEntry::from_bytes("inputs/root.prt", new_root).unwrap();
        seal(&current_directory, &current_job, &mut current_native);
        let mut assembly = base.request.assembly.clone();
        assembly.object.revision = "after-copy".into();
        assembly.object.source_sha256 = sha256(new_root);
        assembly.job = reference(root, &current_directory.join("job.json"));
        let mut object = assembly.object.clone();
        object.source_sha256 = before.object.source_sha256.clone();
        let source_files = prior_job.inputs.iter().filter_map(|f|f.path.as_str().strip_prefix("inputs/")
            .filter(|s| s.ends_with(".prt")).map(|path|json!({"path":path,"bytes":f.identity.bytes,"sha256":f.identity.sha256}))).collect::<Vec<_>>();
        let source_bytes: u64 = source_files
            .iter()
            .map(|f| f["bytes"].as_u64().unwrap())
            .sum();
        let intent = json!({"schema":"oev2.nx-component-copy-intent/1","object":object,
            "source":{"schema":"oev2.nx-assembly-source-closure/1","root_part":"root.prt","max_total_bytes":source_bytes,"files":source_files},
            "source_occurrence_path":["A"],"new_instance_name":"C","source_refs":["independent-synthetic-copy"]});
        let intent_raw = serde_json::to_vec(&intent).unwrap();
        let intent_ref = ArtifactReference {
            path: "copy-intent.json".into(),
            sha256: sha256(&intent_raw),
        };
        let mut job: JobManifest =
            decode(&fs::read(base.creator.join("job.json")).unwrap()).unwrap();
        job.action = "copy_assembly_component".into();
        job.params_schema = "oev2.nx-component-copy-params/1".into();
        job.worker.capabilities = vec![Capability {
            action: job.action.clone(),
            params_schema: job.params_schema.clone(),
        }];
        let params = serde_json::to_vec(
            &json!({"action":job.action,"schema":job.params_schema,"intent":intent_ref,
            "budget":{"max_occurrences":16,"max_prototypes":8,"max_depth":4}}),
        )
        .unwrap();
        fs::write(base.creator.join("params.json"), &params).unwrap();
        fs::write(base.creator.join("inputs/copy-intent.json"), &intent_raw).unwrap();
        job.params = FileEntry::from_bytes("params.json", &params).unwrap();
        job.inputs = vec![FileEntry::from_bytes("inputs/copy-intent.json", &intent_raw).unwrap()];
        for f in &source_files {
            let path = format!("inputs/{}", f["path"].as_str().unwrap());
            let raw = fs::read(prior_directory.join(&path)).unwrap();
            fs::write(base.creator.join(&path), &raw).unwrap();
            job.inputs.push(FileEntry::from_bytes(&path, &raw).unwrap());
        }
        let mut native: NativeResult =
            decode(&fs::read(base.creator.join("native-result.json")).unwrap()).unwrap();
        let mut graph = before_graph;
        for p in graph["prototypes"].as_array_mut().unwrap() {
            p["native_part_tag"] = json!(p["native_part_tag"].as_u64().unwrap() + 1000);
            p["part_path"] = json!(format!(
                "C:/synthetic/copy/{}",
                p["prototype_id"].as_str().unwrap()
            ));
            for b in p["solids"].as_array_mut().unwrap() {
                b["body_tag"] = json!(b["body_tag"].as_u64().unwrap() + 1000);
                b["owning_part_tag"] = json!(b["owning_part_tag"].as_u64().unwrap() + 1000);
            }
        }
        for o in graph["occurrences"].as_array_mut().unwrap() {
            o["part_occurrence_tag"] = json!(o["part_occurrence_tag"].as_u64().unwrap() + 2000);
            o["instance_tag"] = json!(o["instance_tag"].as_u64().unwrap() + 2000);
        }
        let source = &graph["occurrences"][0];
        let root_tag = graph["prototypes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["prototype_id"] == "root.prt")
            .unwrap()["native_part_tag"]
            .clone();
        let proto_tag = graph["prototypes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["prototype_id"] == source["prototype_id"])
            .unwrap()["native_part_tag"]
            .clone();
        let call = json!({"schema":"oev2.nx-component-copy-outcome/1","call_entered":true,"call_returned":true,"wrapper_code":0,"exception_code":0,
            "source_component_tag":source["part_occurrence_tag"],"source_instance_tag":source["instance_tag"],"source_parent_tag":root_tag,"source_prototype_tag":proto_tag,
            "returned_count":1,"returned_component_tags":[9991],"new_instance_tag":9992,"new_parent_tag":root_tag,"new_prototype_tag":proto_tag,"rename_attempted":true,"rename_code":0,"complete":true});
        let mut copied = graph.clone();
        let mut new = source.clone();
        new["name"] = json!("C");
        new["occurrence_id"] = json!("root/000002");
        new["part_occurrence_tag"] = json!(9991);
        new["instance_tag"] = json!(9992);
        copied["occurrences"].as_array_mut().unwrap().push(new);
        native.readback = json!({"action":job.action,"object":object,"intent":intent_ref,"independent_reopen_checked":false,"lifecycle":{"terminate_return":0}});
        for (n,(key,phase,value)) in [("before","component_copy_before",graph.clone()),
            ("before_reference_sets","component_copy_before_reference_sets",sets(&graph)),
            ("call","component_copy_returned",call),("after","component_copy_after",copied.clone()),
            ("after_reference_sets","component_copy_after_reference_sets",sets(&copied)),
            ("saved","component_copy_save_returned",json!({"api":"UF_PART_save_all","return_code":0,"file_error_count":0,"file_errors":[]})),
            ("saved_inventory","component_copy_saved_inventory",copied.clone()),
            ("saved_reference_sets","component_copy_saved_reference_sets",sets(&copied))].into_iter().enumerate() {
            native.readback[key] = serde_json::to_value(record(&base.creator,job.job_id.as_str(),(n+10) as u64,phase,value)).unwrap();
        }
        native.artifacts.clear();
        for f in &source_files {
            let id = f["path"].as_str().unwrap();
            let path = format!("out/build/{id}");
            let raw = fs::read(current_directory.join("inputs").join(id)).unwrap();
            fs::write(base.creator.join(&path), &raw).unwrap();
            native
                .artifacts
                .push(FileEntry::from_bytes(&path, &raw).unwrap());
        }
        seal(&base.creator, &job, &mut native);
        let copy_job = reference(root, &base.creator.join("job.json"));
        let request = CadAssemblyCopyRequest1 {
            schema: "oev2.cad-assembly-copy/1".into(),
            previous_project,
            project: base.request.project.clone(),
            before,
            assembly,
            copy_job,
            copy_native_result_ref: "copy-native".into(),
        };
        let mut f = Self {
            base,
            request,
            prior_directory,
            current_directory,
        };
        f.refresh_current(Some(prior_project));
        f
    }
    pub fn refresh_current(&mut self, original: Option<Value>) {
        let mut p = original.unwrap_or_else(|| self.base.f.read_json(&self.request.project));
        p["revision"] = json!("after-copy");
        p["objects"][&self.request.assembly.object.object_id] =
            json!(self.request.assembly.object.revision);
        p["evidence"]["assembly-model"] = json!(self.request.assembly.object.source_sha256);
        p["evidence"]["assembly-job"] = json!(self.request.assembly.job.sha256);
        p["evidence"][&self.request.assembly.native_result_ref] = json!(
            reference(
                &self.base.f.root,
                &self.current_directory.join("native-result.json")
            )
            .sha256
        );
        p["evidence"]["copy-job"] = json!(self.request.copy_job.sha256);
        let native: NativeResult =
            decode(&fs::read(self.base.creator.join("native-result.json")).unwrap()).unwrap();
        p["evidence"]["copy-intent"] = native.readback["intent"]["sha256"].clone();
        p["evidence"][&self.request.copy_native_result_ref] = json!(
            reference(
                &self.base.f.root,
                &self.base.creator.join("native-result.json")
            )
            .sha256
        );
        self.request.project = self.base.f.write_json("current-copy-project", &p);
        self.request.assembly.project_context =
            definition::context(&p, &self.request.assembly.object);
    }
    pub fn edit_copy(&mut self, edit: impl FnOnce(&Path, &mut JobManifest, &mut NativeResult)) {
        let directory = &self.base.creator;
        let mut job: JobManifest = decode(&fs::read(directory.join("job.json")).unwrap()).unwrap();
        let mut native: NativeResult =
            decode(&fs::read(directory.join("native-result.json")).unwrap()).unwrap();
        edit(directory, &mut job, &mut native);
        seal(directory, &job, &mut native);
        self.request.copy_job = reference(&self.base.f.root, &directory.join("job.json"));
        self.refresh_current(None);
    }
    pub fn edit_event(&mut self, key: &str, edit: impl FnOnce(&mut Value)) {
        self.edit_copy(|directory, _, native| {
            let path = directory.join(native.readback[key]["path"].as_str().unwrap());
            let mut v: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            edit(&mut v);
            let name = format!(
                "receipts/{:04}-{}.json",
                v["sequence"].as_u64().unwrap(),
                v["phase"].as_str().unwrap()
            );
            let raw = serde_json::to_vec(&v).unwrap();
            fs::write(directory.join(&name), &raw).unwrap();
            native.readback[key]["path"] = json!(name);
            native.readback[key]["sha256"] = json!(sha256(raw));
        });
    }
}
