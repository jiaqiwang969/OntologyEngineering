//! Entirely synthetic records, identities and payloads. No historical machine,
//! actual worker, live process or real writer-release authority is represented.
use oe_nx_protocol::*;
use serde::Serialize;
use serde_json::json;
use std::collections::BTreeMap;

fn file(path: &str, content: &[u8]) -> FileEntry {
    FileEntry::from_bytes(path, content).unwrap()
}
fn raw(value: &impl Serialize) -> Vec<u8> {
    serde_json::to_vec(value).unwrap()
}

pub fn records_and_inventory() -> (BTreeMap<String, Vec<u8>>, Vec<FileEntry>) {
    fixture_records(None)
}
pub fn parameter_guard_records() -> (BTreeMap<String, Vec<u8>>, Vec<FileEntry>) {
    fixture_records(Some(
        &json!({"schema":"cad-agent.nx-native-acceptance-params/v2","action":"import_step_bodies","step":"fixture.STEP","expected_units":"mm","maximum_bodies":100000}),
    ))
}
pub fn coherent_parameter_records(
    value: &serde_json::Value,
) -> (BTreeMap<String, Vec<u8>>, Vec<FileEntry>) {
    fixture_records(Some(value))
}
fn fixture_records(
    parameter_value: Option<&serde_json::Value>,
) -> (BTreeMap<String, Vec<u8>>, Vec<FileEntry>) {
    fixture_records_with_role(parameter_value, None)
}
pub fn coherent_worker_role_records(
    worker_id: &str,
    target: &str,
    executable: &str,
    parameters: &serde_json::Value,
) -> (BTreeMap<String, Vec<u8>>, Vec<FileEntry>) {
    fixture_records_with_role(Some(parameters), Some((worker_id, target, executable)))
}
fn fixture_records_with_role(
    parameter_value: Option<&serde_json::Value>,
    role: Option<(&str, &str, &str)>,
) -> (BTreeMap<String, Vec<u8>>, Vec<FileEntry>) {
    let parameter_guard = parameter_value.is_some();
    let root = RootIdentity {
        host_id: "synthetic-writer-host".into(),
        volume_id: "synthetic-volume".into(),
        directory_id: "synthetic-root-directory".into(),
    };
    let mut job = JobManifest {
        capacity: None,
        auxiliary_capacity: None,
        schema: JOB_SCHEMA.into(),
        job_id: JobId::new("phase-synthetic-writer-job").unwrap(),
        root: root.clone(),
        worker: WorkerManifest {
            schema: WORKER_SCHEMA.into(),
            worker_id: "synthetic-worker-not-executable".into(),
            version: "fixture-1".into(),
            target: "x86_64-pc-windows-gnu".into(),
            executable: file("worker/fixture.exe", b"not a native executable"),
            runtime_files: vec![file("worker/helper.dll", b"not a native library")],
            source_files: vec![file("sources/fixture.rs", b"synthetic source identity")],
            build_recipe: file("build/recipe.json", b"synthetic build record"),
            toolchain: "no compiler executed".into(),
            required_runtime: RuntimeIdentity {
                product: "synthetic-runtime".into(),
                product_label: "no installed SDK observed".into(),
                architecture: "x64".into(),
                libraries: vec![file("runtime.dll", b"synthetic SDK identity")],
            },
            capabilities: vec![Capability {
                action: "inspect".into(),
                params_schema: "synthetic.inspect/v1".into(),
            }],
        },
        action: "inspect".into(),
        params_schema: "synthetic.inspect/v1".into(),
        params: file("params.json", br#"{"schema":"synthetic.inspect/v1"}"#),
        inputs: vec![],
    };
    if parameter_guard {
        job.action = parameter_value.unwrap()["action"].as_str().unwrap().into();
        job.params_schema = parameter_value.unwrap()["schema"].as_str().unwrap().into();
        job.worker.capabilities = vec![Capability {
            action: job.action.clone(),
            params_schema: job.params_schema.clone(),
        }];
        job.params = file("params.json", &raw(parameter_value.unwrap()));
    }
    if let Some((worker_id, target, executable)) = role {
        job.job_id = JobId::new("phase-synthetic-role-job").unwrap();
        job.worker.worker_id = worker_id.into();
        job.worker.target = target.into();
        job.worker.executable = file(executable, b"synthetic role fixture; not executable");
    }
    let controller = ProcessIdentity {
        host_id: root.host_id.clone(),
        pid: 10,
        creation_time_100ns: 120_000_000_000_000_010,
        image: BlobIdentity::of(b"synthetic controller image"),
    };
    let worker = ProcessIdentity {
        host_id: root.host_id.clone(),
        pid: 20,
        creation_time_100ns: 120_000_000_000_000_020,
        image: job.worker.executable.identity.clone(),
    };
    let supervisor = ProcessIdentity {
        host_id: root.host_id.clone(),
        pid: 30,
        creation_time_100ns: 120_000_000_000_000_030,
        image: controller.image.clone(),
    };
    let intent = DispatchIntent::new(
        &job,
        controller,
        Digest::of(b"synthetic nonce never used for dispatch"),
    )
    .unwrap();
    let archive = BlobIdentity::of(b"synthetic archive identity; no transport");
    let stage = StageReceipt {
        schema: STAGE_SCHEMA.into(),
        job_id: job.job_id.clone(),
        root,
        manifest_digest: job.digest().unwrap(),
        payload_digest: inventory_digest(&job.payload_files()).unwrap(),
        archive: archive.clone(),
    };
    let mut state = StateRecord::prepared(&job).unwrap();
    state.mark_staged(&job, &stage, &archive).unwrap();
    state.begin_dispatch(&job, intent.clone()).unwrap();
    state.mark_running(&job, worker.clone()).unwrap();
    state.mark_unknown("synthetic_missing_result").unwrap();
    let mut records: BTreeMap<String, Vec<u8>> = BTreeMap::from([
        ("job.json".into(), raw(&job)),
        ("state.json".into(), raw(&state)),
        ("dispatch.json".into(), raw(&intent)),
        ("stage-receipt.json".into(), raw(&stage)),
        (
            "receipts/supervisor-process.json".into(),
            raw(&json!({
                "schema":"cad-agent.nx-supervisor-process/v2",
                "dispatch_id":intent.dispatch_id,
                "process":supervisor,
                "launch_method":"native_COM_Win32_Process_Create"
            })),
        ),
        (
            "receipts/worker-termination.json".into(),
            raw(&json!({
                "schema":"cad-agent.nx-native-termination/v2",
                "dispatch_id":intent.dispatch_id,
                "process":worker,
                "observer":supervisor,
                "exit_code":1
            })),
        ),
        (
            "receipts/0001-source_and_dispatch_verified.json".into(),
            raw(&json!({
                "schema":"cad-agent.nx-native-event/v2",
                "job_id":job.job_id,
                "sequence":1,
                "phase":"source_and_dispatch_verified",
                "observation":{
                    "dispatch_id":intent.dispatch_id,
                    "manifest_digest":job.digest().unwrap(),
                    "worker_manifest_digest":job.worker.digest().unwrap(),
                    "input_digest":job.input_digest().unwrap(),
                    "process":worker,
                    "runtime_files_verified":job.worker.required_runtime,
                    "all_dependency_identities_verified":false
                }
            })),
        ),
        (
            "receipts/supervisor-error.json".into(),
            raw(&json!({
                "schema":"cad-agent.nx-supervisor-error/v2",
                "dispatch_id":intent.dispatch_id,
                "error":"required record missing",
                "worker_killed":false,
                "automatic_retry":false
            })),
        ),
    ]);
    records.insert("receipts/worker.stdout".into(), vec![]);
    records.insert(
        "receipts/worker.stderr".into(),
        b"native_worker_failed:native_loaded_module_path_differs\n".to_vec(),
    );
    assert_eq!(records.len(), CONTROL_RECORDS.len());
    assert!(
        CONTROL_RECORDS
            .iter()
            .all(|name| records.contains_key(*name))
    );
    if parameter_guard {
        records.remove("receipts/0001-source_and_dispatch_verified.json");
        records.insert(
            "receipts/worker.stderr".into(),
            if role.is_some() {
                b"native_worker_failed:native_worker_identity_contract_differs\n".to_vec()
            } else {
                b"native_worker_failed:step_bodies_observation_scope_invalid\n".to_vec()
            },
        );
        records.insert("receipts/worker-launch-environment.json".into(),raw(&json!({"schema":"oev2.nx-worker-launch-environment/1","nx_bin":"C:/NX/NXBIN","child_path_sha256":"a".repeat(64),"job_id":job.job_id,"dispatch_id":intent.dispatch_id,"scope":"worker child PATH only; parent unchanged; NXBIN variable inherited; persisted after worker exit to retain the closed initial-receipt contract","root_cause":"launch-environment hypothesis; sole cause not established"})));
        records.insert("params.json".into(), raw(parameter_value.unwrap()));
    }
    let mut inventory = job.payload_files();
    inventory.extend(
        records
            .iter()
            .filter(|(name, _)| name.as_str() != "params.json")
            .map(|(name, bytes)| file(name, bytes)),
    );
    inventory.sort_by(|a, b| a.path.cmp(&b.path));
    (records, inventory)
}
