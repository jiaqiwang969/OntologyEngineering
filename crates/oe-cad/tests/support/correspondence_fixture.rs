//! Synthetic files only. No worker is launched and no bytes are a real NX part.
//! The test uses the production receipt reader; it cannot construct a verified capability.
use oe_contracts::{
    ArtifactReference, CadCorrespondenceRequest1, CadCorrespondenceState1, CadFeatureBinding1,
    ObjectIdentity, ProjectReviewContext,
};
use oe_core::{SkillRoot, sha256};
use oe_nx_protocol::*;
use serde_json::{Value, json};
use std::{
    cell::Cell,
    fs,
    path::{Path, PathBuf},
};

pub struct Fixture {
    _temporary: Option<tempfile::TempDir>,
    prefix: String,
    pub root: SkillRoot,
    next: Cell<usize>,
}
pub fn observation(name: &str) -> Value {
    let raw = match name {
        "before" => include_str!("../fixtures/correspondence/plate-v1.synthetic-observation.json"),
        "diameter" => {
            include_str!("../fixtures/correspondence/plate-diameter-v2.synthetic-observation.json")
        }
        "deleted" => {
            include_str!("../fixtures/correspondence/plate-deleted-v2.synthetic-observation.json")
        }
        "ambiguous" => {
            include_str!("../fixtures/correspondence/plate-ambiguous-v2.synthetic-observation.json")
        }
        "retagged" => {
            include_str!("../fixtures/correspondence/plate-reopened-v1.synthetic-observation.json")
        }
        "partial" => {
            include_str!("../fixtures/correspondence/plate-partial-v1.synthetic-observation.json")
        }
        _ => panic!("unknown independent fixture"),
    };
    serde_json::from_str(raw).unwrap()
}
pub fn selector(name: &str) -> Value {
    let raw: Value =
        serde_json::from_str(include_str!("../fixtures/correspondence/selectors.json")).unwrap();
    raw[name].clone()
}
pub fn context(project: &Value, object: &ObjectIdentity) -> ProjectReviewContext {
    // Author the documented Project context wire format without adding a new
    // oe-cad -> oe-kernel dependency. CLI tests independently run that owner.
    let projection = json!({
        "project_id":project["project_id"],"revision":project["revision"],
        "purpose":project["purpose"],"objects":project["objects"],
        "conditions":project["conditions"],"evidence":project["evidence"],
        "dependencies":project["dependencies"]
    });
    ProjectReviewContext {
        schema: "oev2.project-review-context/1".into(),
        project_id: project["project_id"].as_str().unwrap().into(),
        project_revision: project["revision"].as_str().unwrap().into(),
        project_sha256: sha256(oe_core::json::canonical_json(&projection).unwrap()),
        object_id: object.object_id.clone(),
        object_revision: object.revision.clone(),
        condition_scope: object.condition_scope.clone(),
        condition_sha256: sha256(
            project["conditions"][&object.condition_scope]
                .as_str()
                .unwrap()
                .as_bytes(),
        ),
        source_sha256: object.source_sha256.clone(),
    }
}
impl Fixture {
    pub fn new() -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let directory = temporary.path().canonicalize().unwrap();
        fs::write(
            directory.join("SKILL.md"),
            "Synthetic correspondence protocol fixture",
        )
        .unwrap();
        fs::create_dir(directory.join("var")).unwrap();
        Self {
            _temporary: Some(temporary),
            prefix: "var".into(),
            root: SkillRoot::new(directory).unwrap(),
            next: Cell::new(0),
        }
    }
    /// Caller selects an exclusive private directory in a real qualified root.
    /// This constructor retains its files for CLI/raw-evidence inspection.
    pub fn in_root(root: SkillRoot, directory: &Path) -> Self {
        let directory = root.private_path(directory).unwrap();
        fs::create_dir_all(directory.parent().unwrap()).unwrap();
        fs::create_dir(&directory).unwrap();
        let prefix = directory
            .strip_prefix(root.path())
            .unwrap()
            .components()
            .map(|c| c.as_os_str().to_str().unwrap())
            .collect::<Vec<_>>()
            .join("/");
        Self {
            _temporary: None,
            root,
            prefix,
            next: Cell::new(0),
        }
    }
    pub fn write_json(&self, label: &str, value: &Value) -> ArtifactReference {
        let next = self.next.get();
        self.next.set(next + 1);
        let relative = format!("{}/{label}-{next}.json", self.prefix);
        let bytes = serde_json::to_vec_pretty(value).unwrap();
        let path = self.root.path().join(&relative);
        // An ordinary fixture write is exclusive. Tampering tests write explicitly.
        let mut options = fs::OpenOptions::new();
        use std::io::Write;
        options
            .create_new(true)
            .write(true)
            .open(path)
            .unwrap()
            .write_all(&bytes)
            .unwrap();
        ArtifactReference {
            path: relative,
            sha256: sha256(&bytes),
        }
    }
    pub fn read_json(&self, reference: &ArtifactReference) -> Value {
        serde_json::from_slice(&fs::read(self.root.path().join(&reference.path)).unwrap()).unwrap()
    }
    pub fn endpoint(
        &self,
        label: &str,
        revision: &str,
        mut part: Value,
        selectors: &[&str],
    ) -> CadCorrespondenceState1 {
        let directory = self.root.path().join(&self.prefix).join(label);
        fs::create_dir(&directory).unwrap();
        // Retagging keeps the same model bytes but a different synthetic job.
        let source = format!("Synthetic plate {revision}; not an NX file").into_bytes();
        let common = b"Synthetic worker and runtime identities; never executed";
        let params = br#"{"schema":"fixture.inspect/v1"}"#;
        let worker = WorkerManifest {
            schema: WORKER_SCHEMA.into(),
            worker_id: "synthetic-never-launched".into(),
            version: "test".into(),
            target: "x86_64-pc-windows-msvc".into(),
            executable: FileEntry::from_bytes("worker/native.exe", common).unwrap(),
            runtime_files: vec![],
            source_files: vec![FileEntry::from_bytes("sources/main.rs", common).unwrap()],
            build_recipe: FileEntry::from_bytes("build/recipe.txt", common).unwrap(),
            toolchain: "fixture-no-compiler".into(),
            required_runtime: RuntimeIdentity {
                product: "NX".into(),
                product_label: "synthetic-not-loaded".into(),
                architecture: "x64".into(),
                libraries: vec![FileEntry::from_bytes("NXBIN/libufun.dll", common).unwrap()],
            },
            capabilities: vec![Capability {
                action: "inspect_detailed".into(),
                params_schema: "fixture.inspect/v1".into(),
            }],
        };
        let job = JobManifest {
            capacity: None,
            auxiliary_capacity: None,
            schema: JOB_SCHEMA.into(),
            job_id: JobId::new(&format!("synthetic-{label}")).unwrap(),
            root: RootIdentity {
                host_id: "fixture-host".into(),
                volume_id: "fixture-volume".into(),
                directory_id: format!("fixture-{label}"),
            },
            worker,
            action: "inspect_detailed".into(),
            params_schema: "fixture.inspect/v1".into(),
            params: FileEntry::from_bytes("params.json", params).unwrap(),
            inputs: vec![FileEntry::from_bytes("inputs/plate.prt", &source).unwrap()],
        };
        for file in job.payload_files() {
            let path = directory.join(file.path.as_str());
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(
                path,
                if file.path.as_str() == "params.json" {
                    params.as_slice()
                } else if file.path.as_str() == "inputs/plate.prt" {
                    source.as_slice()
                } else {
                    common.as_slice()
                },
            )
            .unwrap();
        }
        let process = ProcessIdentity {
            host_id: job.root.host_id.clone(),
            pid: 41,
            creation_time_100ns: 1,
            image: job.worker.executable.identity.clone(),
        };
        let dispatch = DispatchIntent::new(
            &job,
            process.clone(),
            BlobIdentity::of(format!("fixture-{label}").as_bytes()).sha256,
        )
        .unwrap();
        part["path"] = json!("C:/synthetic/inputs/plate.prt");
        let result = NativeResult {
            schema: RESULT_SCHEMA.into(),
            job_id: job.job_id.clone(),
            root: job.root.clone(),
            manifest_digest: job.digest().unwrap(),
            dispatch_id: dispatch.dispatch_id.clone(),
            worker_manifest_digest: job.worker.digest().unwrap(),
            worker: job.worker.executable.identity.clone(),
            input_digest: job.input_digest().unwrap(),
            process: process.clone(),
            runtime: job.worker.required_runtime.clone(),
            action: job.action.clone(),
            outcome: ResultOutcome::Succeeded,
            readback_schema: "fixture.readback/v1".into(),
            readback: json!({"lifecycle":{"terminate_return":0},"parts":[part]}),
            artifacts: vec![],
            errors: vec![],
            unknowns: vec![],
        };
        let mut state = StateRecord::prepared(&job).unwrap();
        state.state = State::Completed;
        state.dispatch = Some(dispatch);
        state.process = Some(process);
        state.result_digest = Some(record_digest(&result).unwrap());
        for (name, value) in [
            ("job.json", serde_json::to_value(&job).unwrap()),
            ("native-result.json", serde_json::to_value(&result).unwrap()),
            ("state.json", serde_json::to_value(&state).unwrap()),
        ] {
            fs::write(directory.join(name), serde_json::to_vec(&value).unwrap()).unwrap();
        }
        let object = ObjectIdentity {
            object_id: "plate".into(),
            revision: revision.into(),
            condition_scope: "geometry-study".into(),
            source_sha256: sha256(&source),
        };
        let features: serde_json::Map<String, Value> = selectors.iter().map(|name| {
            let selector = selector(name);
            let side = if name.starts_with("left") { "left" } else { "right" };
            ((*name).into(), json!({
                "schema":"oev2.cad-feature-intent/1","binding_id":name,"object":object,
                "selector":selector,"occurrence":null,
                "dependency_ids":[format!("drawing:{side}"),format!("fit:{side}"),format!("inspection:{side}")]
            }))
        }).collect();
        let intent = self.write_json(&format!("{label}-intent"), &json!({"features":features}));
        let bindings: Vec<CadFeatureBinding1> = selectors
            .iter()
            .map(|name| {
                serde_json::from_value(json!({"binding_id":name,"intent":{
                    "source":intent,"pointer":format!("/features/{name}")
                }}))
                .unwrap()
            })
            .collect();
        let job_bytes = fs::read(directory.join("job.json")).unwrap();
        let job_ref = ArtifactReference {
            path: format!("{}/{label}/job.json", self.prefix),
            sha256: sha256(&job_bytes),
        };
        let result_ref = format!("native-{label}");
        let project = json!({
            "schema":"oev2.project/1","project_id":"correspondence-software-fixture","revision":revision,
            "purpose":"Frozen software checks; not physical or native execution evidence",
            "objects":{"plate":revision},
            "conditions":{"geometry-study":"Material, load, fit and manufacturing tolerance remain unknown"},
            "evidence":{
                "model":object.source_sha256,"job":job_ref.sha256,"intent":intent.sha256,
                (result_ref.clone()):sha256(fs::read(directory.join("native-result.json")).unwrap())
            },
            "decisions":[],
            "dependencies":{
                "drawing:left":["model"],"fit:left":["model"],"inspection:left":["model"],
                "drawing:right":["model"],"fit:right":["model"],"inspection:right":["model"]
            }
        });
        let project_context = context(&project, &object);
        let project = self.write_json(&format!("{label}-project"), &project);
        CadCorrespondenceState1 {
            project,
            project_context,
            object,
            backend: "nx".into(),
            job: job_ref,
            part_input: "inputs/plate.prt".into(),
            native_result_ref: result_ref,
            bindings,
        }
    }
    pub fn request(
        &self,
        previous: CadCorrespondenceState1,
        mut current: CadCorrespondenceState1,
        declarations: Vec<Value>,
    ) -> CadCorrespondenceRequest1 {
        let source = self.write_json("changes", &json!({"changes":declarations}));
        let count = self.read_json(&source)["changes"].as_array().unwrap().len();
        // New change provenance is registered in the current Project; the previous
        // immutable Project snapshot predates this declaration. It never includes Project bytes,
        // so updating the Project evidence does not form a cyclic digest.
        for state in [&mut current] {
            let mut project = self.read_json(&state.project);
            project["evidence"]["change-original"] = json!(source.sha256);
            state.project_context = context(&project, &state.object);
            state.project = self.write_json("project-with-change", &project);
        }
        serde_json::from_value(json!({
            "schema":"oev2.cad-correspondence/1","previous":previous,"current":current,
            "changes":(0..count).map(|i|json!({"source":source,"pointer":format!("/changes/{i}")})).collect::<Vec<_>>()
        })).unwrap()
    }
    pub fn retained(
        &self,
        previous: CadCorrespondenceState1,
        current: CadCorrespondenceState1,
    ) -> CadCorrespondenceRequest1 {
        assert_eq!(previous.bindings.len(), current.bindings.len());
        let declarations = (0..previous.bindings.len())
            .map(|i| {
                change(
                    &format!("retained-{i}"),
                    "retained",
                    &previous,
                    &current,
                    &[i],
                    &[i],
                )
            })
            .collect();
        self.request(previous, current, declarations)
    }
    pub fn job_directory(&self, state: &CadCorrespondenceState1) -> PathBuf {
        self.root
            .path()
            .join(&state.job.path)
            .parent()
            .unwrap()
            .to_path_buf()
    }
    pub fn native_sha(&self, state: &CadCorrespondenceState1) -> String {
        sha256(fs::read(self.job_directory(state).join("native-result.json")).unwrap())
    }
    pub fn replace_intents(
        &self,
        state: &mut CadCorrespondenceState1,
        edit: impl FnOnce(&mut Value),
    ) {
        let old = &state.bindings[0].intent.source;
        let mut value = self.read_json(old);
        edit(&mut value);
        let source = self.write_json("altered-intent", &value);
        for binding in &mut state.bindings {
            binding.intent.source = source.clone();
        }
        let mut project = self.read_json(&state.project);
        project["evidence"]["intent"] = json!(source.sha256);
        state.project_context = context(&project, &state.object);
        state.project = self.write_json("project-with-intent", &project);
    }
    pub fn change_result(
        &self,
        state: &CadCorrespondenceState1,
        edit: impl FnOnce(&mut NativeResult),
    ) {
        let directory = self.job_directory(state);
        let mut result: NativeResult =
            serde_json::from_slice(&fs::read(directory.join("native-result.json")).unwrap())
                .unwrap();
        edit(&mut result);
        // Update the completion hash so a counterexample reaches the actual
        // protocol rule under test, rather than failing only for stale JSON.
        let mut receipt: StateRecord =
            serde_json::from_slice(&fs::read(directory.join("state.json")).unwrap()).unwrap();
        receipt.result_digest = Some(record_digest(&result).unwrap());
        fs::write(
            directory.join("native-result.json"),
            serde_json::to_vec(&result).unwrap(),
        )
        .unwrap();
        fs::write(
            directory.join("state.json"),
            serde_json::to_vec(&receipt).unwrap(),
        )
        .unwrap();
    }
}
pub fn change(
    id: &str,
    kind: &str,
    previous: &CadCorrespondenceState1,
    current: &CadCorrespondenceState1,
    before: &[usize],
    after: &[usize],
) -> Value {
    json!({
        "schema":"oev2.cad-feature-change/1","change_id":id,"kind":kind,
        "previous":previous.object,"current":current.object,
        "before_bindings":before.iter().map(|i|&previous.bindings[*i]).collect::<Vec<_>>(),
        "after_bindings":after.iter().map(|i|&current.bindings[*i]).collect::<Vec<_>>()
    })
}
