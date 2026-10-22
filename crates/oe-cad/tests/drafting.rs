//! Synthetic protocol tests; these do not claim an NX process was executed.
use oe_cad::drawing::{drafting, intent};
use oe_contracts::{DrawingReferenceIntent, NativeDraftingPlan, ObjectIdentity};
use oe_nx_protocol::*;
use serde_json::json;
use std::path::Path;

fn fixture(directory: &Path) -> ObjectIdentity {
    let source = b"synthetic CAD identity only, not an NX part";
    let common = b"synthetic receipt binding only";
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
        job_id: JobId::new("synthetic-page-binding").unwrap(),
        root: RootIdentity {
            host_id: "fixture-host".into(),
            volume_id: "fixture-volume".into(),
            directory_id: "fixture-root".into(),
        },
        worker,
        action: "inspect_detailed".into(),
        params_schema: "fixture.inspect/v1".into(),
        params: FileEntry::from_bytes("params.json", params).unwrap(),
        inputs: vec![FileEntry::from_bytes("inputs/coupon.prt", source).unwrap()],
    };
    for file in job.payload_files() {
        let path = directory.join(file.path.as_str());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            path,
            if file.path.as_str() == "params.json" {
                params.as_slice()
            } else if file.path.as_str() == "inputs/coupon.prt" {
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
        BlobIdentity::of(b"fixture-dispatch").sha256,
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
        readback_schema: "fixture.readback/v1".into(),
        readback: json!({"lifecycle":{"terminate_return":0},"parts":[{"path":"C:/job/inputs/coupon.prt",
            "unsupported_details":{"status":"unknown","raw_code":-1},"drawing_sheets":[],"annotations":{"dimensions":[]}}]}),
        artifacts: vec![],
        errors: vec![],
        unknowns: vec![],
    };
    let mut state = StateRecord::prepared(&job).unwrap();
    state.state = State::Completed;
    state.dispatch = Some(dispatch);
    state.process = Some(process);
    state.result_digest = Some(record_digest(&result).unwrap());
    std::fs::write(
        directory.join("job.json"),
        serde_json::to_vec(&job).unwrap(),
    )
    .unwrap();
    std::fs::write(
        directory.join("native-result.json"),
        serde_json::to_vec(&result).unwrap(),
    )
    .unwrap();
    std::fs::write(
        directory.join("state.json"),
        serde_json::to_vec(&state).unwrap(),
    )
    .unwrap();
    ObjectIdentity {
        object_id: "coupon".into(),
        revision: "b".into(),
        condition_scope: "design".into(),
        source_sha256: oe_core::sha256(source),
    }
}

#[test]
fn drafting_and_reference_obligations_use_one_complete_original_observer() {
    let temporary = tempfile::tempdir().unwrap();
    let directory = temporary.path().canonicalize().unwrap();
    let object = fixture(&directory);
    let reference: DrawingReferenceIntent = serde_json::from_value(json!({"schema":"oev2.drawing-reference-intent/1",
        "intent_id":"datum","object":object,"local_frame":"part_absolute","unit":"mm","position_tolerance":0.001,
        "proof_scope":"reference_points","source_refs":["independent-design"],"points":[{"point_id":"datum","position":[0,0,0],
            "point_type_raw":1,"line_type_raw":0,"parent_type_raw":3,"parent_subtype_raw":0}]})).unwrap();
    let plan: NativeDraftingPlan = serde_json::from_value(json!({"schema":"oev2.native-drafting-plan/1","sheet_name":"prototype-reference",
        "object":object,"unit":"mm","width":297.0,"height":210.0,"scale_numerator":1.0,"scale_denominator":2.0,
        "projection_angle":"third","model_view_name":"Top","view_center":[148.5,105.0],
        "purpose":"prototype_geometry_only","source_refs":["independent-design"]})).unwrap();
    let raw =
        intent::observe_part(&directory, "inputs/coupon.prt", &object, "native-receipt").unwrap();
    let historical = intent::observe(
        &directory,
        "inputs/coupon.prt",
        &reference,
        "native-receipt",
    )
    .unwrap();
    assert_eq!(raw, historical, "reference /1 wire semantics are preserved");
    let current =
        drafting::observe(&directory, "inputs/coupon.prt", &plan, "native-receipt").unwrap();
    assert_eq!(current.schema, "oev2.native-drawing-observation/2");
    assert_eq!(current.part, raw.part);
    assert_eq!(current.native_result_sha256, raw.native_result_sha256);
    assert_eq!(current.part["unsupported_details"]["status"], "unknown");
    assert_eq!(current.part_input.as_deref(), Some("inputs/coupon.prt"));
    assert_eq!(current.job_directory.as_deref(), directory.to_str());
    assert_ne!(
        drafting::check(&plan, &current.part).unwrap()["conclusion"],
        "pass",
        "completed synthetic receipt alone does not establish a drawing page"
    );
    std::fs::write(directory.join("sources/main.rs"), b"altered binding").unwrap();
    assert!(
        drafting::observe(&directory, "inputs/coupon.prt", &plan, "native-receipt").is_err(),
        "a changed worker source file invalidates both page and reference observation binding"
    );
}
