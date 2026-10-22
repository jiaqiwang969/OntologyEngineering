#![cfg(windows)]
include!("support/native_payload_fixture.rs");
use oe_nx_protocol::{published_stage::*, segmented::TransferPlan};
use oe_windows_controller::{
    adapter, native_stage, transfer_endpoint,
    transfer_reconstruct_windows::PinnedTarget,
    windows::{STATE_NAME, StableRoot},
};
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
fn roundtrip(root: &Path, request: &PublishedStageRequest) -> serde_json::Value {
    let mut child = Command::new(root.parent().unwrap().join("controller-under-test.exe"))
        .arg("--serve-one")
        .arg("--root")
        .arg(root)
        .arg("--nx-root")
        .arg(root.parent().unwrap().join("sdk-not-installed"))
        .arg("--expected-host")
        .arg(&request.job.root.host_id)
        .arg("--expected-volume")
        .arg(&request.job.root.volume_id)
        .arg("--expected-directory")
        .arg(&request.job.root.directory_id)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut bytes = Vec::new();
    write_request(&mut bytes, &request.wire().unwrap(), &[]).unwrap();
    child.stdin.take().unwrap().write_all(&bytes).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let frame = read_frame(&mut &output.stdout[..]).unwrap();
    assert!(frame.payload.is_empty());
    frame.record
}
fn receive(root: &Path, request: &PublishedStageRequest) -> PublishedStageResponse {
    let value = roundtrip(root, request);
    let r: PublishedStageResponse =
        serde_json::from_value(value.clone()).unwrap_or_else(|e| panic!("{e}: {value}"));
    r.validate(request, &request.job.root).unwrap();
    r
}
fn fixture_published() -> (PathBuf, PublishedStageRequest, Vec<Source>) {
    fixture_published_worker(None)
}
fn fixture_published_worker(
    worker: Option<Vec<u8>>,
) -> (PathBuf, PublishedStageRequest, Vec<Source>) {
    let path = std::env::temp_dir().join(format!(
        "oe-published-stage-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(path.join("source")).unwrap();
    std::fs::create_dir(path.join("native")).unwrap();
    let sr = StableRoot::open(&path.join("source"), None).unwrap();
    let nr = StableRoot::open(&path.join("native"), None).unwrap();
    let (mut job, _, mut sources) = fixture();
    job.root = adapter::root_identity(&nr);
    if let Some(worker) = worker {
        job.action = "synthetic.echo".into();
        job.params_schema = "owned.synthetic-params/v1".into();
        job.worker.capabilities = vec![Capability {
            action: job.action.clone(),
            params_schema: job.params_schema.clone(),
        }];
        let params = br#"{"schema":"owned.synthetic-params/v1","sleep_ms":1}"#.to_vec();
        job.params = FileEntry::from_bytes("params.json", &params).unwrap();
        job.worker.executable = FileEntry::from_bytes("worker/fixture.exe", &worker).unwrap();
        job.worker.required_runtime.libraries = vec![
            FileEntry::from_bytes("fixture/runtime.dll", b"unloaded fixture runtime").unwrap(),
        ];
        let sdk = path.join("sdk-not-installed/fixture");
        std::fs::create_dir_all(&sdk).unwrap();
        std::fs::write(sdk.join("runtime.dll"), b"unloaded fixture runtime").unwrap();
        sources[1].data.insert("params.json".into(), params);
        sources[1].data.insert("worker/fixture.exe".into(), worker);
        sources[1].files = sources[1]
            .data
            .iter()
            .map(|(p, b)| FileEntry::from_bytes(p, b).unwrap())
            .collect();
    }
    let mut pubs = Vec::new();
    for (i, source) in sources.iter().enumerate() {
        let budget=serde_json::from_value(json!({"max_total_bytes":104857600,"max_chunk_bytes":104857600,"max_files":10000,"max_chunks":65536})).unwrap();
        let plan = TransferPlan::new(
            JobId::new(format!("source-{i}")).unwrap(),
            adapter::root_identity(&sr),
            "upload".into(),
            Digest::of(b"fixture published bytes"),
            budget,
            source.files.clone(),
        )
        .unwrap();
        let binding = plan.binding().unwrap();
        transfer_endpoint::execute(
            &sr,
            &Request::TransferBegin {
                plan_record: serde_json::to_string(&plan).unwrap(),
            },
            b"",
        )
        .unwrap();
        for (n, span) in plan.spans.iter().enumerate() {
            let file = &plan.files[span.file_index as usize];
            let data = &source.data[file.path.as_str()]
                [span.offset as usize..(span.offset + span.bytes) as usize];
            transfer_endpoint::execute(
                &sr,
                &Request::TransferPut {
                    job_id: plan.job_id.clone(),
                    plan_digest: binding.clone(),
                    index: n as u32,
                    payload: BlobIdentity::of(data),
                },
                data,
            )
            .unwrap();
        }
        let output = JobId::new(format!("output-{i}")).unwrap();
        let rebuilt = transfer_endpoint::execute(
            &sr,
            &Request::TransferReconstruct {
                job_id: plan.job_id.clone(),
                plan_digest: binding.clone(),
                output_id: output.clone(),
            },
            b"",
        )
        .unwrap();
        let published = transfer_endpoint::execute(
            &sr,
            &Request::TransferPublish {
                job_id: plan.job_id,
                plan_digest: binding,
                output_id: output,
                publication_id: JobId::new(format!("publication-{i}")).unwrap(),
                expected_target: rebuilt.reconstructed.unwrap().target,
            },
            b"",
        )
        .unwrap()
        .publication
        .unwrap();
        pubs.push(PublishedPayloadSource {
            publication: published,
            files: source.files.clone(),
            mount: if i == 0 {
                Mount::Inputs
            } else {
                Mount::Payload
            },
        });
    }
    let plan = PublishedPayloadPlan {
        schema: "oev2.nx-published-payload-plan/1".into(),
        job_manifest_digest: job.digest().unwrap(),
        sources: pubs,
    };
    let request = PublishedStageRequest {
        schema: "oev2.nx-published-stage-request/1".into(),
        operation: StageOperation::Prepare,
        job,
        plan,
        source_root_paths: vec![path.join("source").to_str().unwrap().into(); 2],
    };
    request.validate().unwrap();
    let binary = path.join("controller-under-test.exe");
    assert!(!binary.exists());
    std::fs::copy(env!("CARGO_BIN_EXE_oe-nx-controller"), &binary).unwrap();
    assert_eq!(
        std::fs::read(env!("CARGO_BIN_EXE_oe-nx-controller")).unwrap(),
        std::fs::read(&binary).unwrap()
    );
    (path, request, sources)
}
#[test]
fn windows_remote_published_stage_roundtrip_and_recovery() {
    let (path, mut request, data) = fixture_published();
    let native = path.join("native");
    let first = receive(&native, &request);
    assert_eq!(first.state.state, State::Staged);
    let repeated = receive(&native, &request);
    assert_eq!(first, repeated);
    request.operation = StageOperation::Inspect;
    assert_eq!(receive(&native, &request).state, first.state);
    assert!(!native.join("sdk-not-installed").exists());
    assert!(
        !native
            .join(request.job.job_id.as_str())
            .join("dispatch.json")
            .exists()
    );
    request.job.job_id = JobId::new("interrupted-job").unwrap();
    request.plan.job_manifest_digest = request.job.digest().unwrap();
    request.operation = StageOperation::Recover;
    {
        let root = StableRoot::open(&native, None).unwrap();
        let lock = root.lock().unwrap();
        let dir = root
            .create_directory_new(OsStr::new(request.job.job_id.as_str()))
            .unwrap();
        for (i, source) in data.iter().enumerate() {
            for (name, bytes) in &source.data {
                let path = if i == 0 {
                    format!("inputs/{name}")
                } else {
                    name.clone()
                };
                adapter::create_relative_new(&dir, &RelativePath::new(path).unwrap(), bytes)
                    .unwrap();
            }
        }
        for (name, bytes) in [
            ("job.json", serde_json::to_vec(&request.job).unwrap()),
            (
                "source-plan.json",
                serde_json::to_vec(&request.plan).unwrap(),
            ),
        ] {
            dir.create_file_new(OsStr::new(name), &bytes, MAX_RECORD_BYTES as u64)
                .unwrap();
        }
        let prepared = PreparedPayload::for_target(
            &request.job,
            &request.plan,
            PinnedTarget::new(&dir, &lock).unwrap().identity().unwrap(),
        )
        .unwrap();
        assert!(
            native_stage::stage_with_checkpoint(
                &dir,
                &lock,
                &request.job,
                &request.plan,
                &prepared,
                native_stage::Operation::Commit,
                |point| if point == native_stage::Checkpoint::TemporarySynced {
                    Err(std::io::Error::other("test interrupted stage"))
                } else {
                    Ok(())
                }
            )
            .is_err()
        );
    }
    request.operation = StageOperation::Prepare;
    assert_eq!(
        roundtrip(&native, &request)["schema"],
        "cad-agent.nx-error/v2"
    );
    request.operation = StageOperation::Inspect;
    assert_eq!(
        roundtrip(&native, &request)["schema"],
        "cad-agent.nx-error/v2"
    );
    std::fs::create_dir(native.join("other-unresolved-job")).unwrap();
    request.operation = StageOperation::Recover;
    assert_eq!(
        roundtrip(&native, &request)["schema"],
        "cad-agent.nx-error/v2"
    );
    assert!(
        !native
            .join(request.job.job_id.as_str())
            .join(STATE_NAME)
            .exists()
    );
    std::fs::remove_dir(native.join("other-unresolved-job")).unwrap();
    let recovered = receive(&native, &request);
    assert_eq!(recovered.state.state, State::Staged);
    request.operation = StageOperation::Inspect;
    assert_eq!(receive(&native, &request).state, recovered.state);
    let job_dir = native.join(request.job.job_id.as_str());
    assert!(std::fs::read_dir(&job_dir).unwrap().any(|e| {
        e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("stage-recovery-")
    }));
    assert!(!job_dir.join("dispatch.json").exists());
    let before = std::fs::read(job_dir.join(STATE_NAME)).unwrap();
    std::fs::write(path.join("source/output-0/主体.prt"), b"changed source").unwrap();
    assert_eq!(
        roundtrip(&native, &request)["schema"],
        "cad-agent.nx-error/v2"
    );
    assert_eq!(std::fs::read(job_dir.join(STATE_NAME)).unwrap(), before);
    println!(
        "actual_controller_process=prepare_repeat_inspect_recover_verified;other_root_hold_and_source_tamper_rejected;nx_not_started"
    );
    std::fs::remove_dir_all(path).unwrap();
}

fn fetch_call(root: &Path, id: &RootIdentity, request: Request) -> Frame {
    let mut child = Command::new(root.parent().unwrap().join("controller-under-test.exe"))
        .arg("--serve-one")
        .arg("--root")
        .arg(root)
        .arg("--nx-root")
        .arg(root.parent().unwrap().join("sdk-not-installed"))
        .arg("--expected-host")
        .arg(&id.host_id)
        .arg("--expected-volume")
        .arg(&id.volume_id)
        .arg("--expected-directory")
        .arg(&id.directory_id)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut bytes = Vec::new();
    write_request(&mut bytes, &request, &[]).unwrap();
    child.stdin.take().unwrap().write_all(&bytes).unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(result.status.success());
    read_frame(&mut &result.stdout[..]).unwrap()
}
#[test]
fn windows_large_fetch_uses_explicit_auxiliary_capacity() {
    use oe_nx_protocol::large_fetch::*;
    let (path, request, data) = fixture_published();
    let native = path.join("native");
    let mut job = request.job;
    job.job_id = JobId::new("auxiliary-fetch-fixture").unwrap();
    job.auxiliary_capacity = Some(
        serde_json::from_value(
            json!({"max_total_bytes":536870912u64,"max_file_bytes":67108864u64,"max_files":1101}),
        )
        .unwrap(),
    );
    let root = StableRoot::open(&native, None).unwrap();
    let dir = root
        .create_directory_new(OsStr::new(job.job_id.as_str()))
        .unwrap();
    for (i, s) in data.iter().enumerate() {
        for (name, bytes) in &s.data {
            let p = if i == 0 {
                format!("inputs/{name}")
            } else {
                name.clone()
            };
            adapter::create_relative_new(&dir, &RelativePath::new(p).unwrap(), bytes).unwrap();
        }
    }
    dir.create_file_new(
        OsStr::new("job.json"),
        &serde_json::to_vec(&job).unwrap(),
        MAX_RECORD_BYTES as u64,
    )
    .unwrap();
    let receipts = dir.create_directory_new(OsStr::new("receipts")).unwrap();
    for i in 0..1100 {
        receipts
            .create_file_new(
                OsStr::new(&format!("{i:04}-complete.json")),
                &serde_json::to_vec(&json!({"ordinal":i,"original":"complete"})).unwrap(),
                MAX_RECORD_BYTES as u64,
            )
            .unwrap();
    }
    drop(receipts);
    drop(dir);
    drop(root);
    let digest = job.digest().unwrap();
    let get = Request::FetchSnapshot {
        job_id: job.job_id.clone(),
        manifest_digest: digest.clone(),
    };
    let snapshot = FetchSnapshotResponse::decode_frame(
        fetch_call(&native, &job.root, get.clone()),
        &job.root,
        &job.job_id,
        &digest,
    )
    .unwrap();
    let mut received = std::collections::BTreeSet::new();
    for batch in batches(&snapshot, MAX_PAYLOAD_BYTES).unwrap() {
        let frame = fetch_call(
            &native,
            &job.root,
            Request::FetchBatch {
                request_record: serde_json::to_string(&batch).unwrap(),
            },
        );
        let response: FetchBatchResponse = serde_json::from_value(frame.record).unwrap();
        response.validate(&batch, &frame.payload).unwrap();
        let mut offset = 0;
        for slice in batch.slices {
            let end = offset + slice.bytes as usize;
            if slice.file.entry.path.as_str().starts_with("receipts/") {
                assert_eq!(slice.offset, 0);
                assert_eq!(slice.bytes, slice.file.entry.identity.bytes);
                slice
                    .file
                    .entry
                    .identity
                    .verify(&frame.payload[offset..end])
                    .unwrap();
                assert!(received.insert(slice.file.entry.path));
            }
            offset = end;
        }
    }
    assert_eq!(received.len(), 1100);
    assert_eq!(
        FetchSnapshotResponse::decode_frame(
            fetch_call(&native, &job.root, get.clone()),
            &job.root,
            &job.job_id,
            &digest
        )
        .unwrap(),
        snapshot
    );
    let directory = native.join(job.job_id.as_str());
    let extra = directory.join("receipts/extra.json");
    std::fs::write(&extra, b"excess").unwrap();
    assert_eq!(
        fetch_call(&native, &job.root, get).record["schema"],
        "cad-agent.nx-error/v2"
    );
    std::fs::remove_file(extra).unwrap();
    job.auxiliary_capacity = None;
    std::fs::write(
        directory.join("job.json"),
        serde_json::to_vec(&job).unwrap(),
    )
    .unwrap();
    let old = Request::FetchSnapshot {
        job_id: job.job_id.clone(),
        manifest_digest: job.digest().unwrap(),
    };
    assert_eq!(
        fetch_call(&native, &job.root, old).record["schema"],
        "cad-agent.nx-error/v2"
    );
    assert!(!directory.join("dispatch.json").exists());
    println!(
        "actual_auxiliary_fetch=1100_complete_receipts_explicit_v3_bound_legacy_and_excess_rejected_no_nx"
    );
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn windows_large_fetch_batches_and_mutation_checks() {
    use oe_nx_protocol::large_fetch::*;
    use sha2::{Digest as _, Sha256};
    let (path, request, data) = fixture_published();
    let native = path.join("native");
    let mut job = request.job.clone();
    job.job_id = JobId::new("large-fetch-fixture").unwrap();
    let root = StableRoot::open(&native, None).unwrap();
    let dir = root
        .create_directory_new(OsStr::new(job.job_id.as_str()))
        .unwrap();
    for (i, s) in data.iter().enumerate() {
        for (name, bytes) in &s.data {
            let p = if i == 0 {
                format!("inputs/{name}")
            } else {
                name.clone()
            };
            adapter::create_relative_new(&dir, &RelativePath::new(p).unwrap(), bytes).unwrap();
        }
    }
    let size = MAX_PAYLOAD_BYTES + 17;
    let big = dir.path().join("inputs/large.bin");
    let f = std::fs::File::create(&big).unwrap();
    f.set_len(size).unwrap();
    f.sync_all().unwrap();
    drop(f);
    let mut hasher = Sha256::new();
    let zero = [0u8; 65536];
    let mut remaining = size;
    while remaining > 0 {
        let n = remaining.min(zero.len() as u64) as usize;
        hasher.update(&zero[..n]);
        remaining -= n as u64;
    }
    job.inputs.push(FileEntry {
        path: RelativePath::new("inputs/large.bin").unwrap(),
        identity: BlobIdentity {
            sha256: Digest::new(format!("{:x}", hasher.finalize())).unwrap(),
            bytes: size,
        },
    });
    job.validate().unwrap();
    dir.create_file_new(
        OsStr::new("job.json"),
        &serde_json::to_vec(&job).unwrap(),
        MAX_RECORD_BYTES as u64,
    )
    .unwrap();
    dir.create_directory_new(OsStr::new("out")).unwrap();
    let digest = job.digest().unwrap();
    let id = adapter::root_identity(&root);
    drop(dir);
    drop(root);
    let get = Request::FetchSnapshot {
        job_id: job.job_id.clone(),
        manifest_digest: digest.clone(),
    };
    let initial = FetchSnapshotResponse::decode_frame(
        fetch_call(&native, &id, get.clone()),
        &id,
        &job.job_id,
        &digest,
    )
    .unwrap();
    let requests = batches(&initial, 64 * 1024 * 1024).unwrap();
    assert_eq!(requests.len(), 2);
    let mut whole = Sha256::new();
    let mut counted = 0;
    for request in &requests {
        let frame = fetch_call(
            &native,
            &id,
            Request::FetchBatch {
                request_record: serde_json::to_string(request).unwrap(),
            },
        );
        let response: FetchBatchResponse = serde_json::from_value(frame.record).unwrap();
        response.validate(request, &frame.payload).unwrap();
        let mut offset = 0;
        for s in &request.slices {
            let end = offset + s.bytes as usize;
            if s.file.entry.path.as_str() == "inputs/large.bin" {
                whole.update(&frame.payload[offset..end]);
                counted += s.bytes;
            }
            offset = end;
        }
    }
    assert_eq!(counted, size);
    assert_eq!(
        format!("{:x}", whole.finalize()),
        job.inputs.last().unwrap().identity.sha256.as_str()
    );
    let last = FetchSnapshotResponse::decode_frame(
        fetch_call(&native, &id, get.clone()),
        &id,
        &job.job_id,
        &digest,
    )
    .unwrap();
    assert_eq!(initial, last);
    // Same-size mutation can leave physical identity unchanged. A range explicitly
    // does not attest whole-file content; the final full snapshot must change.
    let mut file = std::fs::OpenOptions::new().write(true).open(&big).unwrap();
    file.write_all(b"x").unwrap();
    file.sync_all().unwrap();
    drop(file);
    assert_eq!(
        fetch_call(&native, &id, get.clone()).record["schema"],
        "cad-agent.nx-error/v2"
    );
    let mut file = std::fs::OpenOptions::new().write(true).open(&big).unwrap();
    file.write_all(b"\0").unwrap();
    file.sync_all().unwrap();
    drop(file);
    let mut wrong = requests[0].clone();
    wrong.binding.target.directory_id = "different".into();
    assert_eq!(
        fetch_call(
            &native,
            &id,
            Request::FetchBatch {
                request_record: serde_json::to_string(&wrong).unwrap()
            }
        )
        .record["schema"],
        "cad-agent.nx-error/v2"
    );
    let state = native.join(job.job_id.as_str()).join("state.json");
    std::fs::write(&state, b"partial diagnostic state").unwrap();
    assert_eq!(
        fetch_call(
            &native,
            &id,
            Request::FetchBatch {
                request_record: serde_json::to_string(&requests[0]).unwrap()
            }
        )
        .record["schema"],
        "cad-agent.nx-error/v2"
    );
    std::fs::remove_file(state).unwrap();
    let source = initial
        .files
        .iter()
        .find(|f| f.entry.path.as_str() == "inputs/large.bin")
        .unwrap();
    let single = FetchRange {
        schema: "oev2.nx-fetch-range/1".into(),
        binding: initial.binding.clone(),
        snapshot_digest: initial.digest().unwrap(),
        file: source.clone(),
        offset: 0,
        bytes: 1,
    };
    let old = big.with_extension("retained-original");
    std::fs::rename(&big, &old).unwrap();
    std::fs::copy(&old, &big).unwrap();
    assert_eq!(
        fetch_call(
            &native,
            &id,
            Request::FetchRange {
                request_record: serde_json::to_string(&single).unwrap()
            }
        )
        .record["schema"],
        "cad-agent.nx-error/v2"
    );
    std::fs::remove_file(&big).unwrap();
    std::fs::rename(old, &big).unwrap();
    assert_eq!(
        FetchSnapshotResponse::decode_frame(
            fetch_call(&native, &id, get),
            &id,
            &job.job_id,
            &digest
        )
        .unwrap(),
        initial
    );
    assert!(
        !native
            .join(job.job_id.as_str())
            .join("dispatch.json")
            .exists()
    );
    println!(
        "actual_large_fetch=two_batches_whole_hash_verified_state_directory_file_replacement_rejected"
    );
    println!("large_fetch_fixture={}",serde_json::to_string(&json!({"root":id,"remote_root":native,"job_id":job.job_id,"manifest_digest":digest,"controller_path":path.join("controller-under-test.exe"),"controller":BlobIdentity::of(&std::fs::read(path.join("controller-under-test.exe")).unwrap()),"source_bytes":size,"qualification":"controlled_byte_fixture_no_native_execution"})).unwrap());
}

#[cfg(feature = "acceptance-fixtures")]
#[test]
fn windows_published_dispatch_runs_once_with_bound_origin() {
    let exe = std::env::current_exe().unwrap();
    let worker = exe
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("examples/synthetic-worker-fixture.exe");
    let (path, request, _) = fixture_published_worker(Some(std::fs::read(worker).unwrap()));
    let native = path.join("native");
    let staged = receive(&native, &request);
    assert_eq!(staged.state.state, State::Staged);
    let dir = native.join(request.job.job_id.as_str());
    let history = serde_json::to_vec(&staged.state).unwrap();
    std::fs::write(dir.join("stage-recovery-1-2.json"), &history).unwrap();
    let run = Request::Run {
        job_id: request.job.job_id.clone(),
        manifest_digest: request.job.digest().unwrap(),
        wait_seconds: 30,
    };
    let bad = dir.join("stage-recovery-3-4.json");
    std::fs::write(&bad, b"invalid history").unwrap();
    let refused = fetch_call(&native, &request.job.root, run.clone());
    assert_eq!(refused.record["schema"], "cad-agent.nx-error/v2");
    assert!(!dir.join("dispatch.json").exists());
    assert_eq!(std::fs::read(dir.join("state.json")).unwrap(), history);
    std::fs::remove_file(bad).unwrap();
    let response = fetch_call(&native, &request.job.root, run.clone());
    println!("published_dispatch_response={}", response.record);
    let state: StateRecord =
        serde_json::from_slice(&std::fs::read(dir.join("state.json")).unwrap()).unwrap();
    assert_eq!(state.state, State::Completed, "{}", response.record);
    assert_eq!(state.source_admission, staged.state.source_admission);
    let result: NativeResult =
        serde_json::from_slice(&std::fs::read(dir.join("native-result.json")).unwrap()).unwrap();
    assert_eq!(result.readback["nx_started"], false);
    assert_eq!(
        std::fs::read(dir.join("out/result.bin")).unwrap(),
        b"owned synthetic controller evidence; no NX"
    );
    let before = std::fs::read(dir.join("dispatch.json")).unwrap();
    let refused = fetch_call(&native, &request.job.root, run);
    assert_eq!(refused.record["schema"], "cad-agent.nx-error/v2");
    assert_eq!(std::fs::read(dir.join("dispatch.json")).unwrap(), before);
    assert_eq!(
        std::fs::read(dir.join("stage-recovery-1-2.json")).unwrap(),
        history
    );
    println!("actual_v3_dispatch=single_use_synthetic_process_origin_bound_history_rejected_no_nx");
    std::fs::remove_dir_all(path).unwrap();
}
