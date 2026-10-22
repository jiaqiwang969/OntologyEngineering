//! Synthetic protocol files only, reused by Rust owner and actual CLI controls.
#[path = "occurrence_fixture.rs"]
pub mod occurrence;
use occurrence::{Fixture, definition};
use oe_contracts::{ArtifactReference, CadAssemblyConstructionRequest1, NxAssemblyObservation};
use oe_core::{SkillRoot, sha256};
use oe_nx_protocol::*;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

pub struct ConstructionFixture {
    pub f: Fixture,
    pub request: CadAssemblyConstructionRequest1,
    pub creator: PathBuf,
}
pub fn seal(directory: &Path, job: &JobManifest, native: &mut NativeResult) {
    let dispatch = DispatchIntent::new(
        job,
        native.process.clone(),
        BlobIdentity::of(b"synthetic-construction").sha256,
    )
    .unwrap();
    native.job_id = job.job_id.clone();
    native.root = job.root.clone();
    native.manifest_digest = job.digest().unwrap();
    native.dispatch_id = dispatch.dispatch_id.clone();
    native.worker_manifest_digest = job.worker.digest().unwrap();
    native.worker = job.worker.executable.identity.clone();
    native.input_digest = job.input_digest().unwrap();
    native.runtime = job.worker.required_runtime.clone();
    native.action = job.action.clone();
    let mut state = StateRecord::prepared(job).unwrap();
    state.state = State::Completed;
    state.dispatch = Some(dispatch);
    state.process = Some(native.process.clone());
    state.result_digest = Some(record_digest(native).unwrap());
    for (name, value) in [
        ("job.json", serde_json::to_value(job).unwrap()),
        ("native-result.json", serde_json::to_value(native).unwrap()),
        ("state.json", serde_json::to_value(state).unwrap()),
    ] {
        fs::write(directory.join(name), serde_json::to_vec(&value).unwrap()).unwrap();
    }
}
fn record(
    directory: &Path,
    job: &str,
    sequence: u64,
    phase: &str,
    observation: &Value,
) -> ArtifactReference {
    let name = format!("receipts/{sequence:04}-{phase}.json");
    let raw=serde_json::to_vec(&json!({"schema":"cad-agent.nx-native-event/v2","job_id":job,"sequence":sequence,"phase":phase,"observation":observation})).unwrap();
    let path = directory.join(&name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, &raw).unwrap();
    ArtifactReference {
        path: name,
        sha256: sha256(raw),
    }
}
impl ConstructionFixture {
    pub fn new() -> Self {
        Self::build(Fixture::new())
    }
    pub fn in_root(root: SkillRoot, directory: &Path) -> Self {
        Self::build(Fixture::in_root(root, directory))
    }
    fn build(f: Fixture) -> Self {
        let mut graph = occurrence::direct_graph(0., 0.04);
        for o in graph["occurrences"].as_array_mut().unwrap() {
            o["reference_set"] = json!("Entire Part");
        }
        let endpoint = occurrence::endpoint(
            &f,
            "constructed",
            "a1",
            occurrence::observation("before"),
            graph,
            false,
        );
        let inspector = occurrence::assembly_directory(&f, &endpoint);
        let mut job: JobManifest = decode(&fs::read(inspector.join("job.json")).unwrap()).unwrap();
        let inspector_job = job.clone();
        let inspected: NativeResult =
            decode(&fs::read(inspector.join("native-result.json")).unwrap()).unwrap();
        let creator = inspector.parent().unwrap().join("constructor-job");
        fs::create_dir(&creator).unwrap();
        let mut graph = inspected.readback["assembly"].clone();
        for p in graph["prototypes"].as_array_mut().unwrap() {
            p["native_part_tag"] = json!(p["native_part_tag"].as_u64().unwrap() + 1000);
            p["part_path"] = json!(format!(
                "C:/synthetic/out/build/{}",
                p["prototype_id"].as_str().unwrap()
            ));
            for b in p["solids"].as_array_mut().unwrap() {
                b["owning_part_tag"] = json!(b["owning_part_tag"].as_u64().unwrap() + 1000);
                b["body_tag"] = json!(b["body_tag"].as_u64().unwrap() + 1000);
            }
        }
        for o in graph["occurrences"].as_array_mut().unwrap() {
            o["part_occurrence_tag"] = json!(o["part_occurrence_tag"].as_u64().unwrap() + 2000);
            o["instance_tag"] = json!(o["instance_tag"].as_u64().unwrap() + 2000);
        }
        let plate = fs::read(inspector.join("inputs/plate.prt")).unwrap();
        let mut object = endpoint.assembly.object.clone();
        object.source_sha256 = sha256(b"independent synthetic design source");
        let placements=graph["occurrences"].as_array().unwrap().iter().map(|o|json!({"occurrence_path":[o["name"]],"prototype_id":o["prototype_id"],"local_to_root_m":o["local_to_root_m"]})).collect::<Vec<_>>();
        let intent = json!({"schema":"oev2.nx-assembly-build-intent/1","object":object,"root_part":"root.prt","root_native_units":"mm",
            "files":[{"path":"plate.prt","bytes":plate.len(),"sha256":sha256(&plate)}],"max_total_bytes":1000000,"placements":placements,"source_refs":["independent-synthetic-source"]});
        let intent_raw = serde_json::to_vec(&intent).unwrap();
        let intent_ref = ArtifactReference {
            path: "build-intent.json".into(),
            sha256: sha256(&intent_raw),
        };
        job.job_id = JobId::new("synthetic-construction-creator").unwrap();
        job.action = "create_source_assembly".into();
        job.params_schema = "oev2.nx-source-assembly-params/1".into();
        job.worker.capabilities = vec![Capability {
            action: job.action.clone(),
            params_schema: job.params_schema.clone(),
        }];
        let params = serde_json::to_vec(
            &json!({"schema":job.params_schema,"action":job.action,"intent":intent_ref,
            "budget":{"max_occurrences":16,"max_prototypes":8,"max_depth":4}}),
        )
        .unwrap();
        let payload = BTreeMap::from([
            ("params.json", params.clone()),
            ("inputs/build-intent.json", intent_raw),
            ("inputs/plate.prt", plate),
        ]);
        job.params = FileEntry::from_bytes("params.json", &params).unwrap();
        job.inputs = payload
            .iter()
            .filter(|(n, _)| n.starts_with("inputs/"))
            .map(|(n, b)| FileEntry::from_bytes(n, b).unwrap())
            .collect();
        for file in job.payload_files() {
            let p = creator.join(file.path.as_str());
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            let bytes = payload
                .get(file.path.as_str())
                .cloned()
                .unwrap_or_else(|| fs::read(inspector.join(file.path.as_str())).unwrap());
            fs::write(p, bytes).unwrap();
        }
        let observed: NxAssemblyObservation = serde_json::from_value(graph.clone()).unwrap();
        let root_tag = observed
            .prototypes
            .iter()
            .find(|p| p.prototype_id == observed.root_prototype_id)
            .unwrap()
            .native_part_tag;
        let mut adds = Vec::new();
        for o in &observed.occurrences {
            let p = observed
                .prototypes
                .iter()
                .find(|p| p.prototype_id == o.prototype_id)
                .unwrap();
            adds.push(json!({"schema":"cad-agent.nxopen-entire-loaded-add/v1","wrapper_code":0,"phase":3,"exception_code":0,"direct_owner_matches":1,"entire_set_returned":1,"builder_destroyed":1,
                "outcome":{"normal_return":true,"status_present":true,"status_copied":true,"status_released":true,"unloaded_count":0,"component_tag":o.part_occurrence_tag,"definition_instance_tag":o.instance_tag,"prototype_tag":p.native_part_tag,"definition_parent_tag":root_tag,"reference_set":"Entire Part","entire_reference_set":"Entire Part","complete":true}}));
        }
        let observation_bytes = adds
            .iter()
            .map(|v| serde_json::to_vec(v).unwrap().len())
            .sum::<usize>();
        let batch = json!({"schema":"oev2.nx-load-diagnostic-batch/1","observations":adds,"observation_bytes":observation_bytes,"ordering":"native_emission_order"});
        let add_ref = record(
            &creator,
            job.job_id.as_str(),
            1,
            "native_load_status",
            &batch,
        );
        let add_raw = fs::read(creator.join(&add_ref.path)).unwrap();
        let before = record(
            &creator,
            job.job_id.as_str(),
            2,
            "source_assembly_before_save",
            &graph,
        );
        let after = record(
            &creator,
            job.job_id.as_str(),
            3,
            "source_assembly_after_save",
            &graph,
        );
        let mut artifacts = Vec::new();
        for p in &observed.prototypes {
            let raw = fs::read(inspector.join("inputs").join(&p.prototype_id)).unwrap();
            let name = format!("out/build/{}", p.prototype_id);
            let path = creator.join(&name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, &raw).unwrap();
            artifacts.push(FileEntry::from_bytes(&name, &raw).unwrap());
        }
        let process = ProcessIdentity {
            host_id: job.root.host_id.clone(),
            pid: 43,
            creation_time_100ns: 3,
            image: job.worker.executable.identity.clone(),
        };
        let mut native = NativeResult {
            schema: RESULT_SCHEMA.into(),
            job_id: job.job_id.clone(),
            root: job.root.clone(),
            manifest_digest: job.digest().unwrap(),
            dispatch_id: inspected.dispatch_id.clone(),
            worker_manifest_digest: job.worker.digest().unwrap(),
            worker: job.worker.executable.identity.clone(),
            input_digest: job.input_digest().unwrap(),
            process,
            runtime: job.worker.required_runtime.clone(),
            action: job.action.clone(),
            outcome: ResultOutcome::Succeeded,
            readback_schema: "synthetic-construction-readback/1".into(),
            readback: json!({"action":job.action,"object":object,"intent":intent_ref,"before_save_record":before,"after_save_record":after,
                "save":{"return_code":0,"file_error_count":0,"file_errors":[]},"lifecycle":{"terminate_return":0},
                "load_status_records":[{"schema":"cad-agent.nx-native-event-reference/v2","job_id":job.job_id,"sequence":1,"phase":"native_load_status","record":{"path":add_ref.path,"identity":BlobIdentity::of(&add_raw)}}]}),
            artifacts,
            errors: vec![],
            unknowns: vec![],
        };
        seal(&creator, &job, &mut native);
        let mut result = Self {
            request: CadAssemblyConstructionRequest1 {
                schema: "oev2.cad-assembly-construction/1".into(),
                project: endpoint.definition.project,
                assembly: endpoint.assembly,
                creator_job: ArtifactReference {
                    path: creator
                        .join("job.json")
                        .strip_prefix(f.root.path())
                        .unwrap()
                        .to_string_lossy()
                        .into(),
                    sha256: sha256(fs::read(creator.join("job.json")).unwrap()),
                },
                creator_native_result_ref: "construction-native".into(),
            },
            f,
            creator,
        };
        assert_ne!(job.job_id, inspector_job.job_id);
        result.refresh_project();
        result
    }
    pub fn refresh_project(&mut self) {
        let mut project = self.f.read_json(&self.request.project);
        let native: NativeResult =
            decode(&fs::read(self.creator.join("native-result.json")).unwrap()).unwrap();
        project["evidence"]["construction-job"] = json!(self.request.creator_job.sha256);
        project["evidence"]["construction-intent"] = native.readback["intent"]["sha256"].clone();
        project["evidence"][&self.request.creator_native_result_ref] = json!(sha256(
            fs::read(self.creator.join("native-result.json")).unwrap()
        ));
        self.request.project = self.f.write_json("construction-project", &project);
        self.request.assembly.project_context =
            definition::context(&project, &self.request.assembly.object);
    }
    pub fn edit_creator(&mut self, edit: impl FnOnce(&Path, &mut JobManifest, &mut NativeResult)) {
        let mut job: JobManifest =
            decode(&fs::read(self.creator.join("job.json")).unwrap()).unwrap();
        let mut native: NativeResult =
            decode(&fs::read(self.creator.join("native-result.json")).unwrap()).unwrap();
        edit(&self.creator, &mut job, &mut native);
        seal(&self.creator, &job, &mut native);
        self.request.creator_job.sha256 = sha256(fs::read(self.creator.join("job.json")).unwrap());
        self.refresh_project();
    }
}
