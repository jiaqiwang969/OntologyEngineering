//! Synthetic receipt fixtures only: no native executable or NX part is real.
//! Original receipt/source owners still verify the fixture's declared closure.
#[path = "correspondence_fixture.rs"]
pub mod definition;
pub use definition::{Fixture, observation};
use oe_contracts::{
    ArtifactReference, CadAssemblyUseSource1, CadOccurrenceBindingRequest1, ObjectIdentity,
};
use oe_core::sha256;
use oe_nx_protocol::*;
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::PathBuf};

pub fn identity() -> Vec<f64> {
    vec![
        1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
    ]
}
pub fn pose(x: f64, y: f64, z: f64) -> Vec<f64> {
    let mut p = identity();
    p[3] = x;
    p[7] = y;
    p[11] = z;
    p
}
fn occurrence(
    id: &str,
    parent: Option<&str>,
    name: &str,
    prototype: &str,
    tag: u64,
    local: Vec<f64>,
    global: Vec<f64>,
    parent_scale: f64,
) -> Value {
    let mut raw_parent = local.clone();
    let mut raw_root = global.clone();
    for k in [3, 7, 11] {
        raw_parent[k] /= parent_scale;
        raw_root[k] /= 0.001;
    }
    let csys: Vec<_> = (0..3)
        .flat_map(|c| (0..3).map(move |r| (r, c)))
        .map(|(r, c)| local[r * 4 + c])
        .collect();
    json!({"occurrence_id":id,"parent_occurrence_id":parent,"prototype_id":prototype,
        "part_occurrence_tag":tag,"instance_tag":tag+100,"name":name,"reference_set":"None",
        "raw_parent_origin":[raw_parent[3],raw_parent[7],raw_parent[11]],"raw_parent_csys":csys,
        "raw_parent_transform":raw_parent,"raw_root_transform":raw_root,
        "local_to_parent_m":local,"local_to_root_m":global})
}
fn prototype(id: &str, tag: u64, unit: &str, solid: bool) -> Value {
    let scale = match unit {
        "mm" => 0.001,
        "inch" => 0.0254,
        _ => panic!("fixture unit"),
    };
    let corners = vec![
        -30., -15., 0., 30., -15., 0., 30., 15., 0., -30., 15., 0., -30., -15., 10., 30., -15.,
        10., 30., 15., 10., -30., 15., 10.,
    ];
    let solids = if solid {
        vec![
            json!({"body_tag":tag+500,"owning_part_tag":tag,"is_occurrence":false,
        "exact_box_corners_native":corners,"exact_box_corners_m":corners.iter().map(|v|v*scale).collect::<Vec<_>>(),"volume_m3":null}),
        ]
    } else {
        vec![]
    };
    json!({"prototype_id":id,"part_path":format!("C:/synthetic/inputs/{id}"),"native_part_tag":tag,
        "native_units":unit,"unit_to_m":scale,"load_state_raw":1,"fully_loaded":true,"solids":solids,"sheet_count":0})
}
pub fn direct_graph(a_x: f64, b_x: f64) -> Value {
    json!({"schema":"oev2.nx-native-assembly-observation/1","root_prototype_id":"root.prt",
        "prototypes":[prototype("root.prt",10,"mm",false),prototype("plate.prt",11,"mm",true)],
        "occurrences":[occurrence("root/0",None,"A","plate.prt",20,pose(a_x,0.,0.),pose(a_x,0.,0.),0.001),
            occurrence("root/1",None,"B","plate.prt",21,pose(b_x,0.,0.),pose(b_x,0.,0.),0.001)],
        "complete":true,"source_role":"reference_observation","engineering_adoption":"not_assessed"})
}
pub fn nested_graph() -> Value {
    // Independent literal Rz90: parent .1m x, child +1 inch along parent's x.
    let parent = vec![
        0., -1., 0., 0.1, 1., 0., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
    ];
    let global = vec![
        0., -1., 0., 0.1, 1., 0., 0., 0.0254, 0., 0., 1., 0., 0., 0., 0., 1.,
    ];
    json!({"schema":"oev2.nx-native-assembly-observation/1","root_prototype_id":"root.prt",
        "prototypes":[prototype("root.prt",10,"mm",false),prototype("sub.prt",12,"inch",false),prototype("plate.prt",11,"mm",true)],
        "occurrences":[occurrence("root/0",None,"SUB","sub.prt",20,parent.clone(),parent,0.001),
            occurrence("root/0/0",Some("root/0"),"PLATE","plate.prt",21,pose(0.0254,0.,0.),global,0.0254)],
        "complete":true,"source_role":"reference_observation","engineering_adoption":"not_assessed"})
}
pub fn assembly_directory(f: &Fixture, r: &CadOccurrenceBindingRequest1) -> PathBuf {
    f.root
        .path()
        .join(&r.assembly.job.path)
        .parent()
        .unwrap()
        .to_path_buf()
}
pub fn refresh_project(f: &Fixture, r: &mut CadOccurrenceBindingRequest1, label: &str) {
    let mut p = f.read_json(&r.definition.project);
    p["objects"][&r.assembly.object.object_id] = json!(r.assembly.object.revision);
    p["evidence"]["assembly-model"] = json!(r.assembly.object.source_sha256);
    p["evidence"]["assembly-job"] = json!(r.assembly.job.sha256);
    p["evidence"][&r.assembly.native_result_ref] = json!(sha256(
        fs::read(assembly_directory(f, r).join("native-result.json")).unwrap()
    ));
    for deps in p["dependencies"].as_object_mut().unwrap().values_mut() {
        deps.as_array_mut()
            .unwrap()
            .sort_by_key(|v| v.as_str().unwrap().to_owned());
    }
    r.definition.project_context = definition::context(&p, &r.definition.object);
    r.assembly.project_context = definition::context(&p, &r.assembly.object);
    r.definition.project = f.write_json(label, &p);
}
pub fn endpoint(
    f: &Fixture,
    label: &str,
    revision: &str,
    mut part: Value,
    graph: Value,
    nested: bool,
) -> CadOccurrenceBindingRequest1 {
    part["sheet_count"] = json!(0);
    let selectors = if nested {
        vec!["left_before"]
    } else {
        vec!["left_before", "right_before"]
    };
    let mut definition = f.endpoint(label, "r1", part, &selectors);
    f.replace_intents(&mut definition,|v| {
        for (i,name) in selectors.iter().enumerate() {
            let intent=&mut v["features"][*name];
            let feature=if i==0 {"use:left"}else{"use:right"};
            intent["selector"]=definition::selector("left_before");
            intent["selector"]["feature_id"]=json!(feature);
            let o=if nested {&graph["occurrences"][1]}else{&graph["occurrences"][i]};
            let path=if nested {vec!["SUB","PLATE"]}else{vec![if i==0 {"A"}else{"B"}]};
            intent["occurrence"]=json!({"occurrence_path":path,"prototype_id":"plate.prt","local_to_root_m":o["local_to_root_m"]});
        }
    });
    let definition_dir = f.job_directory(&definition);
    let directory = definition_dir
        .parent()
        .unwrap()
        .join(format!("{label}-assembly"));
    fs::create_dir(&directory).unwrap();
    let mut job: JobManifest = decode(&fs::read(definition_dir.join("job.json")).unwrap()).unwrap();
    job.job_id = JobId::new(&format!("synthetic-{label}-assembly")).unwrap();
    job.root.directory_id = format!("synthetic-{label}-assembly");
    job.action = "inspect_assembly".into();
    // Frozen fixture wire identity, independent of a consumer crate dependency.
    job.params_schema = "cad-agent.nx-native-assembly-params/v1".into();
    job.worker.capabilities = vec![Capability {
        action: job.action.clone(),
        params_schema: job.params_schema.clone(),
    }];
    let params=serde_json::to_vec(&json!({"schema":job.params_schema,"action":"inspect_assembly","root_part":"root.prt",
        "budget":{"max_occurrences":16,"max_prototypes":8,"max_depth":4},"source_role":"reference_observation"})).unwrap();
    job.params = FileEntry::from_bytes("params.json", &params).unwrap();
    let root_bytes =
        format!("Synthetic root assembly {label}/{revision}; never an NX file").into_bytes();
    let mut payload = BTreeMap::<String, Vec<u8>>::new();
    payload.insert("params.json".into(), params);
    payload.insert("inputs/root.prt".into(), root_bytes.clone());
    payload.insert(
        "inputs/plate.prt".into(),
        fs::read(definition_dir.join("inputs/plate.prt")).unwrap(),
    );
    if graph["prototypes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["prototype_id"] == "sub.prt")
    {
        payload.insert(
            "inputs/sub.prt".into(),
            b"Synthetic inch subassembly; never an NX file".to_vec(),
        );
    }
    job.inputs = payload
        .iter()
        .filter(|(p, _)| p.starts_with("inputs/"))
        .map(|(p, b)| FileEntry::from_bytes(p, b).unwrap())
        .collect();
    for file in job.payload_files() {
        let path = directory.join(file.path.as_str());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let bytes = payload
            .get(file.path.as_str())
            .cloned()
            .unwrap_or_else(|| fs::read(definition_dir.join(file.path.as_str())).unwrap());
        fs::write(path, bytes).unwrap();
    }
    let process = ProcessIdentity {
        host_id: job.root.host_id.clone(),
        pid: 42,
        creation_time_100ns: 2,
        image: job.worker.executable.identity.clone(),
    };
    let dispatch = DispatchIntent::new(
        &job,
        process.clone(),
        BlobIdentity::of(label.as_bytes()).sha256,
    )
    .unwrap();
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
        readback_schema: "synthetic-assembly-readback/1".into(),
        readback: json!({"lifecycle":{"terminate_return":0},"assembly":graph,"source_role":"reference_observation","save_requested":false}),
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
    let reference = ArtifactReference {
        path: directory
            .join("job.json")
            .strip_prefix(f.root.path())
            .unwrap()
            .to_string_lossy()
            .into(),
        sha256: sha256(fs::read(directory.join("job.json")).unwrap()),
    };
    let object = ObjectIdentity {
        object_id: "assembly".into(),
        revision: revision.into(),
        condition_scope: definition.object.condition_scope.clone(),
        source_sha256: sha256(&root_bytes),
    };
    let assembly = CadAssemblyUseSource1 {
        object,
        project_context: definition.project_context.clone(),
        job: reference,
        part_input: "inputs/root.prt".into(),
        native_result_ref: format!("assembly-native-{label}"),
    };
    let mut request = CadOccurrenceBindingRequest1 {
        schema: "oev2.cad-occurrence-binding/1".into(),
        definition,
        assembly,
    };
    refresh_project(f, &mut request, &format!("{label}-whole-project"));
    request
}
pub fn edit_intents(
    f: &Fixture,
    r: &mut CadOccurrenceBindingRequest1,
    edit: impl FnOnce(&mut Value),
) {
    f.replace_intents(&mut r.definition, edit);
    refresh_project(f, r, "intent-current-project");
}
pub fn edit_assembly(
    f: &Fixture,
    r: &mut CadOccurrenceBindingRequest1,
    edit: impl FnOnce(&mut Value),
) {
    let directory = assembly_directory(f, r);
    let mut native: NativeResult =
        decode(&fs::read(directory.join("native-result.json")).unwrap()).unwrap();
    edit(&mut native.readback["assembly"]);
    let mut state: StateRecord = decode(&fs::read(directory.join("state.json")).unwrap()).unwrap();
    state.result_digest = Some(record_digest(&native).unwrap());
    fs::write(
        directory.join("native-result.json"),
        serde_json::to_vec(&native).unwrap(),
    )
    .unwrap();
    fs::write(
        directory.join("state.json"),
        serde_json::to_vec(&state).unwrap(),
    )
    .unwrap();
    refresh_project(f, r, "assembly-current-project");
}
