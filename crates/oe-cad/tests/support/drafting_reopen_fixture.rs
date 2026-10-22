//! Authored synthetic protocol/geometry observations only, never an NX result.
//! The page observation constructors reproduce the frozen worker tests/drafting.rs
//! metadata/fixture/direct_fixture input, not a call to a business validator.
#![allow(dead_code)]
use oe_contracts::NativeDraftingPlan;
use oe_nx_protocol::*;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};
const ORIGINAL: &[u8] = b"synthetic original CAD bytes, not an NX part";
const SAVED: &[u8] = b"synthetic saved CAD bytes with page, not an NX part";
const COMMON: &[u8] = b"synthetic worker source closure, never launched";
const PART: &str = "plates/part.prt";
const PARAMS: &str = "cad-agent.nx-native-acceptance-params/v2";
const READBACK: &str = "cad-agent.nx-native-readback/v2";
fn metadata(name: &str) -> Value {
    json!({"type_query":{"return_code":0},
        "owning_part_query":{"return_code":0,"value_valid":true,"matches_expected_owner":true},
        "name_query":{"return_code":0,"value_valid":true,"utf8":name}})
}
fn fixture() -> (NativeDraftingPlan, Value) {
    let plan = serde_json::from_value(json!({"schema":"oev2.native-drafting-plan/1",
        "object":{"object_id":"urn:oev2:test:bracket","revision":"r4","condition_scope":"fixture",
            "source_sha256":"b".repeat(64)},"sheet_name":"TRIAL-REF","unit":"mm",
        "width":320.0,"height":210.0,"scale_numerator":1.0,"scale_denominator":2.0,
        "projection_angle":"first","model_view_name":"TOP","view_center":[150.0,110.0],
        "source_refs":["urn:oev2:test:independent-layout"],"purpose":"prototype_geometry_only"}))
    .unwrap();
    let rotation = json!({"return_code":0,"matrix":[1,0,0,0,1,0,0,0,1]});
    let view = json!({"view_tag":41,"metadata":metadata("TOP@1"),"stale_query_code":0,"stale_after":false,
        "view_type":{"return_code":0,"kind":"drawing_member","subtype":"imported"},
        "owning_drawing":{"return_code":0,"drawing_tag":40},
        "model_part":{"return_code":0,"path":r"C:\controlled\part.prt"},"rotation":rotation,
        "scale":{"return_code":0,"value":0.5},"reference_point":{"return_code":0,"point":[150,110]},
        "borders":{"return_code":0,"values":[110,60,190,130]},
        "visible_objects":{"return_code":0,"capture_complete":true,"visible_count":4,"clipped_count":0}});
    let part = json!({"path":r"C:\controlled\part.prt","units":"mm","solid_count":1,
        "drafting_readback_schema":"oev2.native-drafting-observation/1","drawing_catalog_complete":true,
        "drawing_sheet_count":1,"drawing_sheets":[{"drawing_tag":40,"metadata":metadata("TRIAL-REF"),
            "sheet_info":{"wrapper_code":0,"query_code":0,"value_valid":true,"unit":"mm","size_state":"custom",
                "width":320,"height":210,"scale":0.5,"projection_angle":"first"},
            "view_catalog_complete":true,"views":[view]}],
        "modeling_view_catalog_complete":true,"modeling_views":[{"view_tag":31,"metadata":metadata("TOP"),
            "view_type":{"return_code":0,"kind":"model"},"rotation":rotation}]});
    (plan, part)
}
fn direct_fixture() -> (NativeDraftingPlan, Value) {
    let (plan, mut part) = fixture();
    part["drafting_readback_schema"] = json!("oev2.native-drafting-observation/6");
    part["part_tag"] = json!(50);
    part["solid_tags"] = json!([71]);
    part["bbox"] = json!([[-4, -10, -8, 8, 25, 10]]);
    part["display_part"] =
        json!({"return_code":0,"expected_part":50,"actual_part":50,"matches_expected":true});
    let sheet = &mut part["drawing_sheets"][0];
    sheet["display_open_code"] = json!(0);
    sheet["display_state"] = json!({"set_code":0,"query_code":0,"state_raw":2,
        "current_drawing_code":0,"current_drawing_tag":40,"work_view_code":0,"work_view_tag":42,
        "work_view_type":{"return_code":0,"type_raw":2,"kind":"drawing_sheet"}});
    let view = &mut sheet["views"][0];
    view["boundary"] = json!({"type_query_code":0,"type_raw":4,"kind":"bound_by_objects",
        "objects_query_code":0,"capture_complete":true,"object_count":1,"object_tags":[71]});
    view["visible_objects"]["visible_tags"] = json!([51, 52, 53, 54]);
    view["visible_objects"]["clipped_tags"] = json!([]);
    view["layout_anchor"] = json!({"query_code":0,"point_tag":0,"point_data":null});
    view["layout_reference"] = json!({"schema":"oev2.native-layout-reference/1","unit":"mm",
        "model_part_tag":50,"drawing_view_tag":41,"coordinate_basis":"absolute_current_part_bbox_midpoint",
        "model_coordinates":[2,7.5,1],"drawing_mapping":{"return_code":0,"capture_complete":true,
            "model_point":[2,7.5,1],"coordinates":plan.view_center}});
    part["modeling_views"][0]["view_center"] = json!({"query_code":0,"coordinates":[0,0,0]});
    (plan, part)
}

pub struct Fixture {
    pub saved: PathBuf,
    pub reopened: PathBuf,
    pub plan: NativeDraftingPlan,
}
impl Fixture {
    pub fn new(root: &Path) -> Self {
        // Test-owned directories may arrive through macOS /var symlinks.
        // Select their real path; production path rejection remains unchanged.
        fs::create_dir_all(root).unwrap();
        let root = fs::canonicalize(root).unwrap();
        let (mut plan, mut after) = direct_fixture();
        plan.object.source_sha256 = oe_core::sha256(ORIGINAL);
        let saved = root.join("create");
        let reopened = root.join("reopen");
        let path = format!("C:/synthetic/create/out/{PART}");
        after["path"] = json!(path);
        after["drawing_sheets"][0]["views"][0]["model_part"]["path"] = json!(path);
        after["view_refresh_requested"] = json!(false);
        // Deliberately unresolved geometry/old dimension: page receiving must
        // not turn these unchanged observations into engineering acceptance.
        after["model_edges"] = json!({"schema":"synthetic.unverified-edges/1","complete":false});
        after["annotations"] = json!({"schema":"oev2.nx-annotation-observation/2",
            "units_query":{"return_code":0,"unit":"mm"},
            "dimensions":[{"dimension_tag":80,"retained":true,
                "computed_size":{"value":40,"unit":"mm"},
                "associativity_parents":{"capture_complete":false}}]});
        let mut before = after.clone();
        before["drawing_sheets"] = json!([]);
        before["drawing_sheet_count"] = json!(0);
        let params = json!({"schema":PARAMS,
            "action":"create_drafting_sheet","part":PART,"plan":plan});
        write_job(
            &saved,
            "synthetic-create",
            41,
            ORIGINAL,
            Some(SAVED),
            &params,
            json!({"action":"create_drafting_sheet","lifecycle":{"terminate_return":0},"plan":plan,
                "creation":{"complete":true,"sheet_tag":40},
                "parts":[{"input":PART,"before":before,"after":after,"after_save":after}]}),
        );
        let mut observed = after;
        let path = format!("C:/synthetic/reopen/inputs/{PART}");
        observed["path"] = json!(path);
        observed["drawing_sheets"][0]["views"][0]["model_part"]["path"] = json!(path);
        for pointer in [
            "/part_tag",
            "/solid_tags/0",
            "/display_part/expected_part",
            "/display_part/actual_part",
            "/drawing_sheets/0/drawing_tag",
            "/drawing_sheets/0/display_state/current_drawing_tag",
            "/drawing_sheets/0/display_state/work_view_tag",
            "/drawing_sheets/0/views/0/view_tag",
            "/drawing_sheets/0/views/0/owning_drawing/drawing_tag",
            "/drawing_sheets/0/views/0/boundary/object_tags/0",
            "/drawing_sheets/0/views/0/layout_reference/model_part_tag",
            "/drawing_sheets/0/views/0/layout_reference/drawing_view_tag",
            "/modeling_views/0/view_tag",
            "/annotations/dimensions/0/dimension_tag",
        ] {
            let n = observed.pointer(pointer).unwrap().as_u64().unwrap();
            *observed.pointer_mut(pointer).unwrap() = json!(n + 1000);
        }
        observed["drawing_sheets"][0]["views"][0]["visible_objects"]["visible_tags"] =
            json!([1051, 1052, 1053, 1054]);
        write_job(
            &reopened,
            "synthetic-reopen",
            42,
            SAVED,
            None,
            &json!({"schema":PARAMS,"action":"inspect_detailed","parts":[PART]}),
            json!({"action":"fresh_process_detailed_readback","lifecycle":{"terminate_return":0},"save_requested":false,"parts":[observed]}),
        );
        Self {
            saved,
            reopened,
            plan,
        }
    }
    pub fn edit_result(&self, saved: bool, edit: impl FnOnce(&mut NativeResult)) {
        let root = if saved { &self.saved } else { &self.reopened };
        let job = read::<JobManifest>(&root.join("job.json"));
        let mut result = read::<NativeResult>(&root.join("native-result.json"));
        edit(&mut result);
        seal(root, &job, result);
    }
    pub fn edit_parameters(&self, saved: bool, edit: impl FnOnce(&mut Value)) {
        let root = if saved { &self.saved } else { &self.reopened };
        let mut value = read::<Value>(&root.join("params.json"));
        edit(&mut value);
        let bytes = serde_json::to_vec(&value).unwrap();
        fs::write(root.join("params.json"), &bytes).unwrap();
        let mut job = read::<JobManifest>(&root.join("job.json"));
        job.action = value["action"].as_str().unwrap().into();
        job.params_schema = value["schema"].as_str().unwrap().into();
        job.params = FileEntry::from_bytes("params.json", &bytes).unwrap();
        let result = read::<NativeResult>(&root.join("native-result.json"));
        seal(root, &job, result);
    }
    pub fn use_same_process(&self) {
        let process = read::<NativeResult>(&self.saved.join("native-result.json")).process;
        self.edit_result(false, |r| r.process = process);
    }
}

fn read<T: serde::de::DeserializeOwned>(path: &Path) -> T {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn write_job(
    root: &Path,
    id: &str,
    pid: u32,
    source: &[u8],
    output: Option<&[u8]>,
    params: &Value,
    readback: Value,
) {
    let bytes = serde_json::to_vec(params).unwrap();
    let worker = WorkerManifest {
        schema: WORKER_SCHEMA.into(),
        worker_id: "synthetic-never-launched".into(),
        version: "test".into(),
        target: "x86_64-pc-windows-msvc".into(),
        executable: FileEntry::from_bytes("worker/native.exe", COMMON).unwrap(),
        runtime_files: vec![],
        source_files: vec![FileEntry::from_bytes("sources/main.rs", COMMON).unwrap()],
        build_recipe: FileEntry::from_bytes("build/recipe.txt", COMMON).unwrap(),
        toolchain: "fixture-no-compiler".into(),
        required_runtime: RuntimeIdentity {
            product: "NX".into(),
            product_label: "synthetic-not-loaded".into(),
            architecture: "x64".into(),
            libraries: vec![FileEntry::from_bytes("NXBIN/libufun.dll", COMMON).unwrap()],
        },
        capabilities: [
            "create_drafting_sheet",
            "inspect_detailed",
            "maintain_drawings",
        ]
        .iter()
        .map(|a| Capability {
            action: (*a).into(),
            params_schema: PARAMS.into(),
        })
        .collect(),
    };
    let input = format!("inputs/{PART}");
    let job = JobManifest {
        capacity: None,
        auxiliary_capacity: None,
        schema: JOB_SCHEMA.into(),
        job_id: JobId::new(id).unwrap(),
        root: RootIdentity {
            host_id: "synthetic-host".into(),
            volume_id: "synthetic-volume".into(),
            directory_id: id.into(),
        },
        worker,
        action: params["action"].as_str().unwrap().into(),
        params_schema: params["schema"].as_str().unwrap().into(),
        params: FileEntry::from_bytes("params.json", &bytes).unwrap(),
        inputs: vec![FileEntry::from_bytes(&input, source).unwrap()],
    };
    for entry in job.payload_files() {
        let path = root.join(entry.path.as_str());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            path,
            if entry.path.as_str() == "params.json" {
                bytes.as_slice()
            } else if entry.path.as_str() == input {
                source
            } else {
                COMMON
            },
        )
        .unwrap();
    }
    let artifacts = output
        .map(|raw| {
            let name = format!("out/{PART}");
            let path = root.join(&name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, raw).unwrap();
            vec![FileEntry::from_bytes(&name, raw).unwrap()]
        })
        .unwrap_or_default();
    let process = ProcessIdentity {
        host_id: job.root.host_id.clone(),
        pid,
        creation_time_100ns: u64::from(pid),
        image: job.worker.executable.identity.clone(),
    };
    let result = NativeResult {
        schema: RESULT_SCHEMA.into(),
        job_id: job.job_id.clone(),
        root: job.root.clone(),
        manifest_digest: job.digest().unwrap(),
        dispatch_id: BlobIdentity::of(b"synthetic-dispatch-placeholder").sha256,
        worker_manifest_digest: job.worker.digest().unwrap(),
        worker: job.worker.executable.identity.clone(),
        input_digest: job.input_digest().unwrap(),
        process,
        runtime: job.worker.required_runtime.clone(),
        action: job.action.clone(),
        outcome: ResultOutcome::Succeeded,
        readback_schema: READBACK.into(),
        readback,
        artifacts,
        errors: vec![],
        unknowns: vec![],
    };
    seal(root, &job, result);
}
fn seal(root: &Path, job: &JobManifest, mut result: NativeResult) {
    let dispatch = DispatchIntent::new(
        job,
        result.process.clone(),
        BlobIdentity::of(b"synthetic-dispatch").sha256,
    )
    .unwrap();
    result.manifest_digest = job.digest().unwrap();
    result.input_digest = job.input_digest().unwrap();
    result.worker_manifest_digest = job.worker.digest().unwrap();
    result.action = job.action.clone();
    result.dispatch_id = dispatch.dispatch_id.clone();
    let mut state = StateRecord::prepared(job).unwrap();
    state.state = State::Completed;
    state.dispatch = Some(dispatch);
    state.process = Some(result.process.clone());
    state.result_digest = Some(record_digest(&result).unwrap());
    for (name, value) in [
        ("job.json", serde_json::to_value(job).unwrap()),
        ("native-result.json", serde_json::to_value(&result).unwrap()),
        ("state.json", serde_json::to_value(&state).unwrap()),
    ] {
        fs::write(root.join(name), serde_json::to_vec(&value).unwrap()).unwrap();
    }
}
