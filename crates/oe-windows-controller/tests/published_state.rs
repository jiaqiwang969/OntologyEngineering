use oe_nx_protocol::{published_payload::*, *};
use serde_json::json;
fn file(path: &str, content: &[u8]) -> FileEntry {
    FileEntry::from_bytes(path, content).unwrap()
}
fn root() -> RootIdentity {
    RootIdentity {
        host_id: "fixture-host".into(),
        volume_id: "volume-1".into(),
        directory_id: "directory-1".into(),
    }
}
fn runtime() -> RuntimeIdentity {
    RuntimeIdentity {
        product: "fixture-native-runtime".into(),
        product_label: "not-an-NX-acceptance".into(),
        architecture: "x64".into(),
        libraries: vec![file("fixture.dll", b"fixture DLL identity")],
    }
}
fn job() -> JobManifest {
    JobManifest {
        capacity: None,
        auxiliary_capacity: None,
        schema: JOB_SCHEMA.into(),
        job_id: JobId::new("fixture-job").unwrap(),
        root: root(),
        worker: WorkerManifest {
            schema: WORKER_SCHEMA.into(),
            worker_id: "fixture-worker".into(),
            version: "1".into(),
            target: "x86_64-pc-windows-gnu".into(),
            executable: file("worker/fixture.exe", b"not an executable"),
            runtime_files: vec![file("worker/helper.dll", b"not a DLL")],
            source_files: vec![file("sources/main.rs", b"fixture source")],
            build_recipe: file("build/recipe.json", b"fixture recipe"),
            toolchain: "fixture-toolchain".into(),
            required_runtime: runtime(),
            capabilities: vec![
                Capability {
                    action: "inspect".into(),
                    params_schema: "fixture.inspect/v1".into(),
                },
                Capability {
                    action: "custom.thread_gauge".into(),
                    params_schema: "fixture.gauge/v1".into(),
                },
            ],
        },
        action: "inspect".into(),
        params_schema: "fixture.inspect/v1".into(),
        params: file(
            "params.json",
            br#"{"schema":"fixture.inspect/v1","units":"mm"}"#,
        ),
        inputs: vec![
            file("inputs/主体.prt", b"not a PRT"),
            file("inputs/子装配/轴.prt", b"not another PRT"),
        ],
    }
}

fn capacity() -> NativeCapacity {
    serde_json::from_value(json!({"max_input_bytes":268435456,"max_input_file_bytes":209715200,"max_output_bytes":536870912,"max_output_file_bytes":268435456,"max_input_files":10000,"max_output_files":10000,"max_segment_bytes":104857600})).unwrap()
}
fn large() -> JobManifest {
    let mut j = job();
    j.schema = LARGE_JOB_SCHEMA.into();
    j.capacity = Some(capacity());
    j
}

fn fixture() -> (JobManifest, PublishedPayloadPlan, PreparedPayload) {
    let j = large();
    let mut files = j.payload_files();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let mut source = j.root.clone();
    source.directory_id = "source-root".into();
    let mut tree = source.clone();
    tree.directory_id = "files-root".into();
    let mut publication = source.clone();
    publication.directory_id = "publication-root".into();
    let mut target = j.root.clone();
    target.directory_id = "task-directory".into();
    let p = transfer::SourcePublication {
        schema: "oev2.nx-source-publication/1".into(),
        source_root: source,
        job_id: JobId::new("source-job").unwrap(),
        plan_digest: Digest::of(b"plan"),
        output_id: JobId::new("source-output").unwrap(),
        publication_id: JobId::new("source-publication").unwrap(),
        publication_root: publication,
        target: tree,
        inventory_digest: record_digest(&files).unwrap(),
        files: files.len(),
        bytes: files.iter().map(|f| f.identity.bytes).sum(),
        state: "published".into(),
        native_execution_started: false,
    };
    let plan = PublishedPayloadPlan {
        schema: "oev2.nx-published-payload-plan/1".into(),
        job_manifest_digest: j.digest().unwrap(),
        sources: vec![PublishedPayloadSource {
            publication: p,
            files: files.clone(),
            mount: Mount::Payload,
        }],
    };
    plan.validate(&j).unwrap();
    let prepared = PreparedPayload {
        schema: "oev2.nx-prepared-payload/1".into(),
        job_manifest_digest: j.digest().unwrap(),
        source_plan_digest: plan.digest(&j).unwrap(),
        target,
        payload_digest: record_digest(&files).unwrap(),
        files: files.len(),
        bytes: files.iter().map(|f| f.identity.bytes).sum(),
        job_staged: false,
        native_execution_started: false,
    };
    prepared.validate(&j, &plan).unwrap();
    (j, plan, prepared)
}
fn intent(j: &JobManifest) -> DispatchIntent {
    DispatchIntent::new(
        j,
        ProcessIdentity {
            host_id: j.root.host_id.clone(),
            pid: 20,
            creation_time_100ns: 100,
            image: BlobIdentity::of(b"fixture controller"),
        },
        Digest::of(b"fixture dispatch"),
    )
    .unwrap()
}
#[test]
fn published_state_requires_explicit_origin_and_dispatch_review() {
    let (job, plan, prepared) = fixture();
    let mut state = StateRecord::prepared(&job).unwrap();
    assert_eq!(state.schema, PUBLISHED_STATE_SCHEMA);
    assert!(
        classify_hold(&HoldInput::StateBytes(serde_json::to_vec(&state).unwrap())).blocks_writes
    );
    let mut invalid = state.clone();
    invalid.state = State::Staged;
    assert!(invalid.validate(&job).is_err());
    state.mark_staged_published(&job, &plan, &prepared).unwrap();
    state
        .validate_published_stage(&job, &plan, &prepared, &prepared.target)
        .unwrap();
    assert!(state.begin_dispatch(&job, intent(&job)).is_err());
    let old = state.clone();
    state
        .begin_published_dispatch(&job, &plan, &prepared, &prepared.target, intent(&job))
        .unwrap();
    assert_eq!(state.source_admission, old.source_admission);
    assert_eq!(state.state, State::Starting);
    assert!(
        state
            .begin_published_dispatch(&job, &plan, &prepared, &prepared.target, intent(&job))
            .is_err()
    );
    assert!(state.mark_staged_published(&job, &plan, &prepared).is_err());
}
#[test]
fn published_state_rejects_swapped_origins_directories_and_versions() {
    let (job, plan, prepared) = fixture();
    let mut state = StateRecord::prepared(&job).unwrap();
    state.mark_staged_published(&job, &plan, &prepared).unwrap();
    let mut mixed = StateRecord::prepared(&job).unwrap();
    mixed.schema = STATE_SCHEMA.into();
    assert!(mixed.validate(&job).is_err());
    let legacy = super_legacy_job();
    let mut mixed = StateRecord::prepared(&legacy).unwrap();
    mixed.schema = PUBLISHED_STATE_SCHEMA.into();
    assert!(mixed.validate(&legacy).is_err());
    let mut wrong = plan.clone();
    wrong.sources[0].publication.plan_digest = Digest::of(b"other source same content");
    wrong.validate(&job).unwrap();
    let mut receipt = prepared.clone();
    receipt.source_plan_digest = wrong.digest(&job).unwrap();
    receipt.validate(&job, &wrong).unwrap();
    assert!(
        state
            .validate_published_stage(&job, &wrong, &receipt, &receipt.target)
            .is_err()
    );
    let mut directory = prepared.target.clone();
    directory.directory_id = "replacement".into();
    assert!(
        state
            .validate_published_stage(&job, &plan, &prepared, &directory)
            .is_err()
    );
    let mut invalid = state.clone();
    invalid.schema = STATE_SCHEMA.into();
    assert!(invalid.validate_shape().is_err());
    let mut invalid = state.clone();
    invalid.source_admission = None;
    assert!(invalid.validate_shape().is_err());
    let mut old = job.clone();
    old.schema = JOB_SCHEMA.into();
    old.capacity = None;
    old.validate().unwrap();
    assert!(state.validate(&old).is_err());
    let mut receipt = prepared.clone();
    receipt.job_staged = true;
    assert!(
        state
            .validate_published_stage(&job, &plan, &receipt, &receipt.target)
            .is_err()
    );
}
#[test]
fn published_state_status_preserves_unknown_and_legacy_bytes() {
    let old = job();
    let state = StateRecord::prepared(&old).unwrap();
    let raw = serde_json::to_value(&state).unwrap();
    assert!(raw.get("source_admission").is_none());
    assert_eq!(
        raw,
        json!({"schema":STATE_SCHEMA,"job_id":old.job_id,"root":old.root,"manifest_digest":old.digest().unwrap(),"state":"PREPARED","dispatch":null,"process":null,"result_digest":null,"reason":null,"uncertainty_history":[]})
    );
    assert!(
        !classify_hold(&HoldInput::StateBytes(serde_json::to_vec(&state).unwrap())).blocks_writes
    );
    let (j, _, _) = fixture();
    let pending = StateRecord::prepared(&j).unwrap();
    let raw = serde_json::to_vec(&pending).unwrap();
    let h = classify_hold(&HoldInput::StateBytes(raw.clone()));
    assert!(h.blocks_writes);
    let mut reply = StatusResponse {
        schema: STATUS_RESPONSE_SCHEMA.into(),
        job_id: j.job_id.clone(),
        root: j.root.clone(),
        state: serde_json::to_value(&pending).unwrap(),
        state_file: BlobIdentity::of(&raw),
        blocks_writes: false,
        hold_reason: h.reason.clone(),
        legacy_read_only: false,
        automatic_retry: false,
    };
    assert!(reply.validate_binding(&j.job_id, &j.root).is_err());
    reply.blocks_writes = true;
    reply.validate_binding(&j.job_id, &j.root).unwrap();
    reply.validate_hold(&h).unwrap();
    let (job, plan, prepared) = fixture();
    let mut state = StateRecord::prepared(&job).unwrap();
    state.mark_staged_published(&job, &plan, &prepared).unwrap();
    let binding = state.source_admission.clone();
    state
        .begin_published_dispatch(&job, &plan, &prepared, &prepared.target, intent(&job))
        .unwrap();
    state.mark_unknown("fixture observation lost").unwrap();
    assert_eq!(state.source_admission, binding);
    assert_eq!(state.uncertainty_history.len(), 1);
    state
        .validate_published_stage(&job, &plan, &prepared, &prepared.target)
        .unwrap();
    let bytes = serde_json::to_vec(&state).unwrap();
    let hold = classify_hold(&HoldInput::StateBytes(bytes.clone()));
    assert!(hold.blocks_writes && !hold.legacy_v1);
    assert_eq!(hold.reason, "unresolved_v3_dispatch");
    let status = StatusResponse {
        schema: STATUS_RESPONSE_SCHEMA.into(),
        job_id: job.job_id.clone(),
        root: job.root.clone(),
        state: serde_json::to_value(&state).unwrap(),
        state_file: BlobIdentity::of(&bytes),
        blocks_writes: true,
        hold_reason: hold.reason.clone(),
        legacy_read_only: false,
        automatic_retry: false,
    };
    status.validate_binding(&job.job_id, &job.root).unwrap();
    status.verify_state_bytes(&bytes).unwrap();
    status.validate_hold(&hold).unwrap();
}
#[cfg(windows)]
#[test]
fn windows_published_state_atomic_checkpoints_remain_held() {
    use oe_windows_controller::windows::*;
    use std::ffi::OsStr;
    let path = std::env::temp_dir().join(format!(
        "oe-v3-state-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&path).unwrap();
    {
        let root = StableRoot::open(&path, None).unwrap();
        let lock = root.lock().unwrap();
        let (job, plan, prepared) = fixture();
        let mut state = StateRecord::prepared(&job).unwrap();
        state.mark_staged_published(&job, &plan, &prepared).unwrap();
        let bytes = serde_json::to_vec(&state).unwrap();
        // Atomic-storage fixture only. Protocol bindings are synthetic, not a claim
        // that the real root contains an admitted engineering payload.
        for (i, stop) in [
            WriteCheckpoint::PendingSynced,
            WriteCheckpoint::TemporarySynced,
            WriteCheckpoint::ReplacedAndReadback,
        ]
        .into_iter()
        .enumerate()
        {
            let dir = root
                .create_directory_new(OsStr::new(&format!("checkpoint-{i}")))
                .unwrap();
            assert!(
                write_state_with_checkpoint(&dir, &lock, None, &bytes, |at| if at == stop {
                    Err(std::io::Error::other("injected stop"))
                } else {
                    Ok(())
                })
                .is_err()
            );
            assert!(dir.read_record(OsStr::new(PENDING_NAME)).unwrap().is_some());
            assert!(classify_hold(&HoldInput::PendingWrite).blocks_writes);
            recover_state_new(&dir, &lock, &bytes).unwrap();
            let actual = dir.read_record(OsStr::new(STATE_NAME)).unwrap().unwrap();
            let observed: StateRecord = decode(&actual).unwrap();
            assert_eq!(observed, state);
            observed
                .validate_published_stage(&job, &plan, &prepared, &prepared.target)
                .unwrap();
            assert!(dir.read_record(OsStr::new(PENDING_NAME)).unwrap().is_none());
            println!("v3_state_checkpoint_{i}=explicitly_recovered");
        }
    }
    std::fs::remove_dir_all(path).unwrap();
}

fn super_legacy_job() -> JobManifest {
    job()
}
