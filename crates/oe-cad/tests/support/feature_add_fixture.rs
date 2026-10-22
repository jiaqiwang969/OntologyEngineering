//! Authored software fixtures, never real NX parts or native execution evidence.
//! Receipt construction reuses the existing fixtures; all receiving checks are
//! performed by the production owners. Geometry literals precede native trials.
#[path = "copy_fixture.rs"]
pub mod copy;
pub use copy::construction::legacy::occurrence::definition;
use copy::construction::legacy::{occurrence, seal};
use oe_contracts::{
    ArtifactReference, CadAssemblyUseSource1, CadCorrespondenceRequest1, CadCorrespondenceState1,
    CadFeatureAddRequest1,
};
use oe_core::{SkillRoot, sha256};
use oe_nx_protocol::*;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub struct AddFixture {
    pub f: definition::Fixture,
    pub correspondence: CadCorrespondenceRequest1,
    pub request: CadFeatureAddRequest1,
    pub add_directory: PathBuf,
    pub prior_assembly_directory: PathBuf,
    pub current_assembly_directory: PathBuf,
}

pub fn circle(left: bool) -> Value {
    json!({"feature_id":if left {"hole:left"} else {"hole:right"},"local_frame":"part_absolute",
        "center":[if left {-20.} else {20.},0.,8.],"normal":[0.,0.,1.],"radius":6.,"unit":"mm",
        "center_tolerance":1e-6,"radius_tolerance":1e-6,"angular_tolerance_radians":1e-5,"full_circle":true})
}
fn volume(deleted: bool) -> f64 {
    // Independent analytic plate minus one/two radius-6 through cylinders, SI.
    let holes = if deleted { 1. } else { 2. };
    (80. * 40. * 8. - holes * std::f64::consts::PI * 36. * 8.) * 1e-9
}
pub fn detail(deleted: bool) -> Value {
    let mut v = definition::observation("before");
    let mut edges = Vec::new();
    for (side, x) in [(0, -20.), (1, 20.)] {
        if deleted && side == 0 {
            continue;
        }
        for (level, z) in [(0, 0.), (1, 8.)] {
            let mut edge = v["model_edges"]["entries"][side].clone();
            edge["body_tag"] = json!(511);
            edge["edge_tag"] = json!(101 + side * 2 + level);
            edge["arc"]["center"] = json!([x, 0., z]);
            edge["arc"]["radius"] = json!(6.);
            edges.push(edge);
        }
    }
    v["part_tag"] = json!(11);
    v["solid_tags"] = json!([511]);
    v["sheet_count"] = json!(0);
    v["bbox"] = json!([[-40., -20., 0., 40., 20., 8.]]);
    // Raw native frame/dimensions and the separately authored expected box are
    // both retained. Do not compute this oracle with ExactBounds::new.
    v["exact_work_csys_bounds"] = json!([{
        "min_corner":[-40.,-20.,0.],"directions":[[1.,0.,0.],[0.,1.,0.],[0.,0.,1.]],
        "distances":[80.,40.,8.],"absolute_axis_bounds":[-40.,-20.,0.,40.,20.,8.]
    }]);
    v["model_edges"]["bodies"][0]["body_tag"] = json!(511);
    v["model_edges"]["bodies"][0]["edge_tags"] = json!(
        edges
            .iter()
            .map(|e| e["edge_tag"].clone())
            .collect::<Vec<_>>()
    );
    v["model_edges"]["entries"] = json!(edges);
    let extents = [-40., -20., 0., 40., 20., 8.];
    let mut faces = Vec::new();
    for axis in 0..3 {
        for side in 0..2 {
            let value = extents[axis + 3 * side];
            let mut point = [0.; 3];
            point[axis] = value;
            let mut dir = [0.; 3];
            dir[axis] = 1.;
            let mut bounds = extents;
            bounds[axis] = value;
            bounds[axis + 3] = value;
            faces.push(json!({"face_tag":601+axis*2+side,"body_tag":511,"query_code":0,"surface_type":22,
                "axis_point":point,"axis_direction":dir,"face_bounds":bounds,"radius":0.,"normal_direction":1}));
        }
    }
    for (side, x) in [(0, -20.), (1, 20.)] {
        if deleted && side == 0 {
            continue;
        }
        faces.push(json!({"face_tag":607+side,"body_tag":511,"query_code":0,"surface_type":16,
            "axis_point":[x,0.,0.],"axis_direction":[0.,0.,1.],"face_bounds":[x-6.,-6.,0.,x+6.,6.,8.],
            "radius":6.,"normal_direction":-1}));
    }
    v["solid_face_inventory_complete"] = json!(true);
    v["solid_face_inventory"] = json!(faces);
    v
}
fn graph(deleted: bool, target_path: &str) -> Value {
    let mut graph = occurrence::direct_graph(0., 0.1);
    graph["prototypes"][1]["prototype_id"] = json!(target_path);
    graph["prototypes"][1]["part_path"] = json!(format!("C:/synthetic/inputs/{target_path}"));
    for occurrence in graph["occurrences"].as_array_mut().unwrap() {
        occurrence["prototype_id"] = json!(target_path);
    }
    let corners = [
        -40., -20., 0., 40., -20., 0., 40., 20., 0., -40., 20., 0., -40., -20., 8., 40., -20., 8.,
        40., 20., 8., -40., 20., 8.,
    ];
    graph["prototypes"][1]["solids"][0]["exact_box_corners_native"] = json!(corners);
    graph["prototypes"][1]["solids"][0]["exact_box_corners_m"] = json!(corners.map(|v| v * 0.001));
    graph["prototypes"][1]["solids"][0]["volume_m3"] = json!(volume(deleted));
    graph
}
fn sets(graph: &Value) -> Value {
    json!({"schema":"oev2.nx-assembly-reference-sets/2","api":"UF_ASSEM_ask_component_data",
        "root_prototype_id":"root.prt","complete":true,
        "members":graph["occurrences"].as_array().unwrap().iter().map(|o|json!({
            "occurrence_id":o["occurrence_id"],"part_occurrence_tag":o["part_occurrence_tag"],
            "instance_tag":o["instance_tag"],"instance_reference_set":"None","part_occurrence_reference_set":"None"})).collect::<Vec<_>>()})
}
fn decode_job(dir: &Path) -> JobManifest {
    decode(&fs::read(dir.join("job.json")).unwrap()).unwrap()
}
fn decode_native(dir: &Path) -> NativeResult {
    decode(&fs::read(dir.join("native-result.json")).unwrap()).unwrap()
}
fn write_payload(dir: &Path, source: &Path, job: &JobManifest, replacements: &[(&str, Vec<u8>)]) {
    for file in job.payload_files() {
        let bytes = replacements
            .iter()
            .find(|(name, _)| *name == file.path.as_str())
            .map(|(_, b)| b.clone())
            .unwrap_or_else(|| fs::read(source.join(file.path.as_str())).unwrap());
        let target = dir.join(file.path.as_str());
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, bytes).unwrap();
    }
}
fn assembly(
    f: &definition::Fixture,
    state: &mut CadCorrespondenceState1,
    label: &str,
    deleted: bool,
    pid: u32,
    target_path: &str,
) -> (CadAssemblyUseSource1, PathBuf) {
    let source = f.job_directory(state);
    let directory = source.parent().unwrap().join(format!("{label}-assembly"));
    fs::create_dir(&directory).unwrap();
    let mut job = decode_job(&source);
    let mut native = decode_native(&source);
    job.job_id = JobId::new(&format!("synthetic-{label}-assembly")).unwrap();
    job.action = "inspect_assembly".into();
    job.params_schema = "oev2.nx-assembly-reference-sets-params/2".into();
    job.worker.capabilities = vec![Capability {
        action: job.action.clone(),
        params_schema: job.params_schema.clone(),
    }];
    let params = serde_json::to_vec(&json!({"schema":job.params_schema,"action":job.action,"root_part":"root.prt",
        "capture_reference_sets":true,"budget":{"max_occurrences":16,"max_prototypes":8,"max_depth":4},"source_role":"reference_observation"})).unwrap();
    let root_bytes = format!("Synthetic {label} root; not a native NX part").into_bytes();
    let plate = fs::read(source.join("inputs/plate.prt")).unwrap();
    let target_input = format!("inputs/{target_path}");
    job.params = FileEntry::from_bytes("params.json", &params).unwrap();
    job.inputs = vec![
        FileEntry::from_bytes("inputs/root.prt", &root_bytes).unwrap(),
        FileEntry::from_bytes(&target_input, &plate).unwrap(),
    ];
    write_payload(
        &directory,
        &source,
        &job,
        &[
            ("params.json", params),
            ("inputs/root.prt", root_bytes.clone()),
            (&target_input, plate),
        ],
    );
    native.process.pid = pid;
    native.process.creation_time_100ns = u64::from(pid);
    let graph = graph(deleted, target_path);
    native.readback = json!({"lifecycle":{"terminate_return":0},"assembly":graph,"reference_sets":sets(&graph),
        "source_role":"reference_observation","save_requested":false});
    seal(&directory, &job, &mut native);
    let mut object = state.object.clone();
    object.object_id = "assembly".into();
    object.source_sha256 = sha256(&root_bytes);
    let reference = copy::reference(&f.root, &directory.join("job.json"));
    let native_ref = format!("assembly-native-{label}");
    let mut p = f.read_json(&state.project);
    p["objects"]["assembly"] = json!(object.revision);
    p["evidence"]["assembly-model"] = json!(object.source_sha256);
    p["evidence"]["assembly-job"] = json!(reference.sha256);
    p["evidence"][&native_ref] =
        json!(copy::reference(&f.root, &directory.join("native-result.json")).sha256);
    state.project_context = definition::context(&p, &state.object);
    state.project = f.write_json("assembly-project", &p);
    (
        CadAssemblyUseSource1 {
            object: object.clone(),
            project_context: definition::context(&p, &object),
            job: reference,
            part_input: "inputs/root.prt".into(),
            native_result_ref: native_ref,
        },
        directory,
    )
}
impl AddFixture {
    pub fn new() -> Self {
        Self::build(definition::Fixture::new(), "plate.prt")
    }
    pub fn same_leaf_target() -> Self {
        Self::build(definition::Fixture::new(), "parts/root.prt")
    }
    pub fn in_root(root: SkillRoot, directory: &Path) -> Self {
        Self::build(definition::Fixture::in_root(root, directory), "plate.prt")
    }
    pub fn in_root_same_leaf_target(root: SkillRoot, directory: &Path) -> Self {
        Self::build(
            definition::Fixture::in_root(root, directory),
            "parts/root.prt",
        )
    }
    fn build(f: definition::Fixture, target_path: &str) -> Self {
        let mut previous = f.endpoint("add-before", "r1", detail(true), &["right_before"]);
        let mut current = f.endpoint(
            "add-after",
            "r2",
            detail(false),
            &["left_before", "right_before"],
        );
        for state in [&mut previous, &mut current] {
            f.replace_intents(state, |v| {
                for (name, intent) in v["features"].as_object_mut().unwrap() {
                    intent["selector"] = circle(name == "left_before");
                    intent["occurrence"] = Value::Null;
                }
            });
        }
        let current_detail = f.job_directory(&current);
        let job = decode_job(&current_detail);
        let mut native = decode_native(&current_detail);
        native.process.pid = 45;
        native.process.creation_time_100ns = 45;
        seal(&current_detail, &job, &mut native);
        let mut p = f.read_json(&current.project);
        p["evidence"][&current.native_result_ref] = json!(f.native_sha(&current));
        current.project_context = definition::context(&p, &current.object);
        current.project = f.write_json("current-detail-project", &p);
        let (before_assembly, prior_assembly_directory) =
            assembly(&f, &mut previous, "add-before", true, 42, target_path);
        let (assembly, current_assembly_directory) =
            assembly(&f, &mut current, "add-after", false, 44, target_path);
        let changes = vec![
            definition::change("new-left", "new", &previous, &current, &[], &[0]),
            definition::change(
                "retained-right",
                "retained",
                &previous,
                &current,
                &[0],
                &[1],
            ),
        ];
        let correspondence = f.request(previous, current, changes);
        let add_directory = prior_assembly_directory.parent().unwrap().join("add-job");
        fs::create_dir(&add_directory).unwrap();
        let source_job = decode_job(&prior_assembly_directory);
        let files=source_job.inputs.iter().map(|e|json!({"path":e.path.as_str().strip_prefix("inputs/").unwrap(),"bytes":e.identity.bytes,"sha256":e.identity.sha256})).collect::<Vec<_>>();
        let intent = json!({"schema":"oev2.nx-feature-add-intent/1","object":correspondence.previous.object,
            "source":{"schema":"oev2.nx-assembly-source-closure/1","root_part":"root.prt","max_total_bytes":files.iter().map(|v|v["bytes"].as_u64().unwrap()).sum::<u64>(),"files":files},
            "target_part":target_path,"target":circle(true),"retained":circle(false),"feature_type":"CYLINDER",
            "feature_location_mm":[-20.,0.,-1.],"tool_end_z_mm":9.,"plate_bounds_mm":[-40.,-20.,0.,40.,20.,8.],"source_refs":["synthetic-original-intent"]});
        let raw = serde_json::to_vec(&intent).unwrap();
        let intent_ref = ArtifactReference {
            path: "add-intent.json".into(),
            sha256: sha256(&raw),
        };
        let mut job = source_job.clone();
        job.job_id = JobId::new("synthetic-add-job").unwrap();
        job.action = "add_hole_feature".into();
        job.params_schema = "oev2.nx-feature-add-params/1".into();
        job.worker.capabilities = vec![Capability {
            action: job.action.clone(),
            params_schema: job.params_schema.clone(),
        }];
        let params = serde_json::to_vec(
            &json!({"schema":job.params_schema,"action":job.action,"intent":intent_ref,
            "budget":{"max_occurrences":16,"max_prototypes":8,"max_depth":4}}),
        )
        .unwrap();
        job.params = FileEntry::from_bytes("params.json", &params).unwrap();
        job.inputs
            .push(FileEntry::from_bytes("inputs/add-intent.json", &raw).unwrap());
        write_payload(
            &add_directory,
            &prior_assembly_directory,
            &job,
            &[("params.json", params), ("inputs/add-intent.json", raw)],
        );
        let mut native = decode_native(&prior_assembly_directory);
        native.process.pid = 43;
        native.process.creation_time_100ns = 43;
        native.readback = json!({"action":job.action,"object":correspondence.previous.object,"intent":intent_ref,
            "target_part":target_path,"independent_reopen_checked":false,"engineering_adoption":"not_assessed","lifecycle":{"terminate_return":0}});
        let inventory = json!({"schema":"oev2.nx-feature-inventory/1","part_tag":11,"body_tag":511,"complete":true,"features":[
            {"feature_tag":701,"owning_part_tag":11,"body_tag":511,"is_occurrence":false,"feature_type":"BLOCK","location_mm":[-40.,-20.,0.],"face_tags":[601,602,603,604,605,606],"query_codes":[0,0,0,0,0],"complete":true},
            {"feature_tag":702,"owning_part_tag":11,"body_tag":511,"is_occurrence":false,"feature_type":"CYLINDER","location_mm":[-20.,0.,-1.],"face_tags":[607],"query_codes":[0,0,0,0,0],"complete":true},
            {"feature_tag":703,"owning_part_tag":11,"body_tag":511,"is_occurrence":false,"feature_type":"CYLINDER","location_mm":[20.,0.,-1.],"face_tags":[608],"query_codes":[0,0,0,0,0],"complete":true}]});
        let mut before_features = inventory.clone();
        before_features["features"]
            .as_array_mut()
            .unwrap()
            .remove(1);
        let call = json!({"schema":"oev2.nx-feature-add-outcome/1","part_tag":11,"body_tag":511,"feature_tag":702,
            "origin_mm":[-20.,0.,-1.],"height_mm":10.,"diameter_mm":12.,"call_entered":true,"call_returned":true,
            "return_code":0,"update_code":0,"complete":true});
        let lineage = json!({"feature_tag":702,"part_tag":11,"body_tag":511,"target_feature_id":"hole:left",
            "previous_feature_count":2,"current_feature_count":3,"persistent_topology_identity":"not_established"});
        let rows = [
            (10, "before", "before", graph(true, target_path)),
            (
                11,
                "before_reference_sets",
                "before_reference_sets",
                sets(&graph(true, target_path)),
            ),
            (12, "before_detail", "before_detail", detail(true)),
            (13, "before_features", "before_features", before_features),
            (15, "call", "returned", call),
            (16, "after_detail", "after_detail", detail(false)),
            (17, "after", "after", graph(false, target_path)),
            (18, "after_features", "after_features", inventory.clone()),
            (19, "lineage", "lineage", lineage),
            (
                20,
                "after_reference_sets",
                "after_reference_sets",
                sets(&graph(false, target_path)),
            ),
            (
                22,
                "saved",
                "save_returned",
                json!({"api":"UF_PART_save_all","return_code":0,"file_error_count":0,"file_errors":[]}),
            ),
            (23, "saved_detail", "saved_detail", detail(false)),
            (24, "saved_features", "saved_features", inventory),
            (
                25,
                "saved_inventory",
                "saved_inventory",
                graph(false, target_path),
            ),
            (
                26,
                "saved_reference_sets",
                "saved_reference_sets",
                sets(&graph(false, target_path)),
            ),
        ];
        for (seq, key, phase, value) in rows {
            native.readback[key] = serde_json::to_value(copy::record(
                &add_directory,
                job.job_id.as_str(),
                seq,
                &format!("feature_add_{phase}"),
                value,
            ))
            .unwrap();
        }
        copy::record(
            &add_directory,
            job.job_id.as_str(),
            14,
            "feature_add_pending",
            json!({"intent":intent_ref,"target_part":target_path,"part_tag":11,"body_tag":511}),
        );
        copy::record(
            &add_directory,
            job.job_id.as_str(),
            21,
            "feature_add_save_pending",
            json!({"namespace":"C:/synthetic/out/build"}),
        );
        native.artifacts.clear();
        for name in ["root.prt", target_path] {
            let raw = fs::read(current_assembly_directory.join("inputs").join(name)).unwrap();
            let target = format!("out/build/{name}");
            fs::create_dir_all(add_directory.join(&target).parent().unwrap()).unwrap();
            fs::write(add_directory.join(&target), &raw).unwrap();
            native
                .artifacts
                .push(FileEntry::from_bytes(&target, &raw).unwrap());
        }
        seal(&add_directory, &job, &mut native);
        let request = CadFeatureAddRequest1 {
            schema: "oev2.cad-feature-add/1".into(),
            correspondence: f.write_json(
                "correspondence-initial",
                &serde_json::to_value(&correspondence).unwrap(),
            ),
            before_assembly,
            assembly,
            add_job: copy::reference(&f.root, &add_directory.join("job.json")),
            add_native_result_ref: "addition-native".into(),
            change_id: "new-left".into(),
        };
        let mut out = Self {
            f,
            correspondence,
            request,
            add_directory,
            prior_assembly_directory,
            current_assembly_directory,
        };
        out.refresh();
        out
    }
    pub fn refresh(&mut self) {
        let native = decode_native(&self.add_directory);
        self.request.add_job = copy::reference(&self.f.root, &self.add_directory.join("job.json"));
        let mut p = self.f.read_json(&self.correspondence.current.project);
        p["evidence"]["addition-job"] = json!(self.request.add_job.sha256);
        p["evidence"]["addition-intent"] = native.readback["intent"]["sha256"].clone();
        p["evidence"][&self.request.add_native_result_ref] = json!(
            copy::reference(&self.f.root, &self.add_directory.join("native-result.json")).sha256
        );
        self.correspondence.current.project = self.f.write_json("add-current-project", &p);
        self.persist_correspondence();
    }
    pub fn persist_correspondence(&mut self) {
        let old = self.f.read_json(&self.correspondence.previous.project);
        let new = self.f.read_json(&self.correspondence.current.project);
        self.correspondence.previous.project_context =
            definition::context(&old, &self.correspondence.previous.object);
        self.correspondence.current.project_context =
            definition::context(&new, &self.correspondence.current.object);
        self.request.before_assembly.project_context =
            definition::context(&old, &self.request.before_assembly.object);
        self.request.assembly.project_context =
            definition::context(&new, &self.request.assembly.object);
        self.request.correspondence = self.f.write_json(
            "addition-correspondence",
            &serde_json::to_value(&self.correspondence).unwrap(),
        );
    }
    pub fn edit_projects(&mut self, edit: impl FnOnce(&mut Value, &mut Value)) {
        let mut old = self.f.read_json(&self.correspondence.previous.project);
        let mut new = self.f.read_json(&self.correspondence.current.project);
        edit(&mut old, &mut new);
        self.correspondence.previous.project = self.f.write_json("edited-previous-project", &old);
        self.correspondence.current.project = self.f.write_json("edited-current-project", &new);
        self.persist_correspondence();
    }
    pub fn edit_add(&mut self, edit: impl FnOnce(&Path, &mut JobManifest, &mut NativeResult)) {
        let mut job = decode_job(&self.add_directory);
        let mut native = decode_native(&self.add_directory);
        edit(&self.add_directory, &mut job, &mut native);
        seal(&self.add_directory, &job, &mut native);
        self.refresh();
    }
    pub fn edit_event(&mut self, key: &str, edit: impl FnOnce(&mut Value)) {
        self.edit_add(|dir, _, native| {
            let p = dir.join(native.readback[key]["path"].as_str().unwrap());
            let mut v: Value = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
            edit(&mut v);
            let name = format!(
                "receipts/{:04}-{}.json",
                v["sequence"].as_u64().unwrap(),
                v["phase"].as_str().unwrap()
            );
            let raw = serde_json::to_vec(&v).unwrap();
            fs::write(dir.join(&name), &raw).unwrap();
            native.readback[key] = json!({"path":name,"sha256":sha256(raw)});
        });
    }
}
