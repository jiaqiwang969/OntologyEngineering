include!("support/native_payload_fixture.rs");
#[test]
fn complete_payload_includes_worker_parameters_and_inputs() {
    let (job, plan, mut sources) = fixture();
    let mut refs = sources
        .iter_mut()
        .map(|s| s as &mut dyn CopySource)
        .collect::<Vec<_>>();
    let mut target = Sink::default();
    let r = prepare(&job, &plan, &mut refs, &mut target).unwrap();
    r.validate(&job, &plan).unwrap();
    assert!(!r.job_staged && !r.native_execution_started);
    assert_eq!(r.files, job.payload_files().len());
    assert_eq!(target.data.len(), job.payload_files().len() + 2);
    assert_eq!(
        decode::<JobManifest>(&target.data["job.json"]).unwrap(),
        job
    );
    assert_eq!(
        decode::<PublishedPayloadPlan>(&target.data["source-plan.json"]).unwrap(),
        plan
    );
    assert!(target.data.contains_key("worker/fixture.exe"));
    assert!(target.data.contains_key("params.json"));
    assert!(target.data.contains_key("inputs/主体.prt"));
    assert!(prepare(&job, &plan, &mut refs, &mut target).is_err());
    let mut bad = r.clone();
    bad.job_staged = true;
    assert!(bad.validate(&job, &plan).is_err());
    let mut bad = r;
    bad.source_plan_digest = Digest::of(b"other");
    assert!(bad.validate(&job, &plan).is_err());
}
#[test]
fn complete_payload_rejects_mappings_and_bad_parameters_before_write() {
    for mode in 0..5 {
        let (mut job, mut plan, mut sources) = fixture();
        match mode {
            0 => {
                plan.sources.pop();
            }
            1 => {
                plan.sources.push(plan.sources[0].clone());
            }
            2 => {
                plan.sources[0].mount = Mount::Payload;
            }
            3 => {
                sources[0].record.state = "pending".into();
            }
            _ => {
                let bad = br#"{"schema":"wrong/v1"}"#.to_vec();
                job.params = file("params.json", &bad);
                sources[1].data.insert("params.json".into(), bad);
                sources[1] = source("program-source", sources[1].data.clone());
                plan.sources[1].publication = sources[1].record.clone();
                plan.sources[1].files = sources[1].files.clone();
                plan.job_manifest_digest = job.digest().unwrap();
                plan.validate(&job).unwrap();
            }
        }
        let mut refs = sources
            .iter_mut()
            .map(|s| s as &mut dyn CopySource)
            .collect::<Vec<_>>();
        let mut target = Sink::default();
        assert!(prepare(&job, &plan, &mut refs, &mut target).is_err());
        assert!(target.data.is_empty());
    }
}
#[test]
fn complete_payload_preserves_partial_tree_and_rejects_source_change() {
    for mode in 0..2 {
        let (job, plan, mut sources) = fixture();
        sources[0].fail_stream = mode == 0;
        sources[0].fail_after = mode == 1;
        let mut target = Sink::default();
        {
            let mut refs = sources
                .iter_mut()
                .map(|s| s as &mut dyn CopySource)
                .collect::<Vec<_>>();
            assert!(prepare(&job, &plan, &mut refs, &mut target).is_err());
        }
        assert_eq!(
            decode::<JobManifest>(&target.data["job.json"]).unwrap(),
            job
        );
        assert_eq!(
            decode::<PublishedPayloadPlan>(&target.data["source-plan.json"]).unwrap(),
            plan
        );
        let before = target.data.clone();
        sources[0].fail_stream = false;
        sources[0].fail_after = false;
        let mut refs = sources
            .iter_mut()
            .map(|s| s as &mut dyn CopySource)
            .collect::<Vec<_>>();
        assert!(prepare(&job, &plan, &mut refs, &mut target).is_err());
        assert_eq!(target.data, before);
    }
}
#[cfg(windows)]
#[test]
fn windows_complete_payload_uses_two_published_trees() {
    use oe_nx_protocol::segmented::TransferPlan;
    use oe_windows_controller::{
        adapter,
        native_input_copy_windows::PublishedSource,
        transfer_publication::{Operation, publish},
        transfer_reconstruct_windows::PinnedTarget,
        windows::StableRoot,
    };
    use std::ffi::OsStr;
    let path = std::env::temp_dir().join(format!(
        "oe-payload-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(path.join("source")).unwrap();
    std::fs::create_dir(path.join("native")).unwrap();
    {
        let sr = StableRoot::open(&path.join("source"), None).unwrap();
        let nr = StableRoot::open(&path.join("native"), None).unwrap();
        let sl = sr.lock().unwrap();
        let nl = nr.lock().unwrap();
        let (mut j, _, data) = fixture();
        j.root = adapter::root_identity(&nr);
        let mut plans = Vec::new();
        let mut publications = Vec::new();
        let mut outputs = Vec::new();
        let mut metadata = Vec::new();
        for (i, s) in data.iter().enumerate() {
            let out_name = format!("output-{i}");
            let pub_name = format!("published-{i}");
            let output = sr.create_directory_new(OsStr::new(&out_name)).unwrap();
            for (name, b) in &s.data {
                adapter::create_relative_new(&output, &RelativePath::new(name.clone()).unwrap(), b)
                    .unwrap();
            }
            let budget=serde_json::from_value(json!({"max_total_bytes":104857600,"max_chunk_bytes":104857600,"max_files":10000,"max_chunks":65536})).unwrap();
            let plan = TransferPlan::new(
                JobId::new(format!("source-{i}")).unwrap(),
                adapter::root_identity(&sr),
                "upload".into(),
                Digest::of(b"fixture source"),
                budget,
                s.files.clone(),
            )
            .unwrap();
            let id = PinnedTarget::new(&output, &sl).unwrap().identity().unwrap();
            let publication = publish(
                &sr,
                &sl,
                &plan,
                &JobId::new(&out_name).unwrap(),
                &JobId::new(&pub_name).unwrap(),
                &id,
                Operation::Publish,
            )
            .unwrap();
            metadata.push(sr.directory(OsStr::new(&pub_name)).unwrap());
            outputs.push(output);
            plans.push(plan);
            publications.push(publication);
        }
        let mut sources = Vec::new();
        for i in 0..2 {
            sources.push(
                PublishedSource::open(
                    &sr,
                    &sl,
                    &outputs[i],
                    &metadata[i],
                    &plans[i],
                    &publications[i],
                )
                .unwrap(),
            );
        }
        let plan = PublishedPayloadPlan {
            schema: "oev2.nx-published-payload-plan/1".into(),
            job_manifest_digest: j.digest().unwrap(),
            sources: (0..2)
                .map(|i| PublishedPayloadSource {
                    publication: publications[i].clone(),
                    files: plans[i].files.clone(),
                    mount: if i == 0 {
                        Mount::Inputs
                    } else {
                        Mount::Payload
                    },
                })
                .collect(),
        };
        let output = nr
            .create_directory_new(OsStr::new(j.job_id.as_str()))
            .unwrap();
        let mut target = PinnedTarget::new(&output, &nl).unwrap();
        let mut refs = sources
            .iter_mut()
            .map(|s| s as &mut dyn CopySource)
            .collect::<Vec<_>>();
        let receipt = prepare(&j, &plan, &mut refs, &mut target).unwrap();
        receipt.validate(&j, &plan).unwrap();
        assert!(!receipt.job_staged);
        assert!(
            std::fs::OpenOptions::new()
                .write(true)
                .open(output.path().join("params.json"))
                .is_err()
        );
        assert!(prepare(&j, &plan, &mut refs, &mut target).is_err());
    }
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn published_stage_wire_binds_operation_origin_and_no_execution() {
    use oe_nx_protocol::published_stage::*;
    let (job, plan, mut sources) = fixture();
    let mut sink = Sink::default();
    let mut refs = sources
        .iter_mut()
        .map(|s| s as &mut dyn CopySource)
        .collect::<Vec<_>>();
    let prepared = prepare(&job, &plan, &mut refs, &mut sink).unwrap();
    let mut state = StateRecord::prepared(&job).unwrap();
    state.mark_staged_published(&job, &plan, &prepared).unwrap();
    let request = PublishedStageRequest {
        schema: "oev2.nx-published-stage-request/1".into(),
        operation: StageOperation::Prepare,
        job,
        plan,
        source_root_paths: vec![r"C:\fixture-source".into(); 2],
    };
    request.validate().unwrap();
    let wire = request.wire().unwrap();
    let mut bytes = Vec::new();
    write_request(&mut bytes, &wire, &[]).unwrap();
    assert_eq!(read_request(&mut &bytes[..]).unwrap().0, wire);
    assert!(write_request(&mut Vec::new(), &wire, b"unexpected").is_err());
    let response = PublishedStageResponse {
        schema: "oev2.nx-published-stage-response/1".into(),
        request_digest: record_digest(&request).unwrap(),
        root: request.job.root.clone(),
        prepared,
        state,
        native_execution_started: false,
    };
    response.validate(&request, &request.job.root).unwrap();
    let mut changed = request.clone();
    changed.operation = StageOperation::Recover;
    assert!(response.validate(&changed, &changed.job.root).is_err());
    let mut bad = response.clone();
    bad.native_execution_started = true;
    assert!(bad.validate(&request, &request.job.root).is_err());
    for path in [
        r"\\server\share",
        r"C:relative",
        r"C:\a\..\b",
        r"C:\a\.\b",
        "/tmp/source",
        "C:\\bad\nroot",
    ] {
        let mut bad = request.clone();
        bad.source_root_paths[0] = path.into();
        assert!(bad.validate().is_err(), "{path}");
    }
    let mut bad = request;
    bad.source_root_paths.pop();
    assert!(bad.validate().is_err());
}

fn byte_snapshot() -> oe_nx_protocol::large_fetch::FetchSnapshot {
    use oe_nx_protocol::large_fetch::*;
    let (job, plan, mut sources) = fixture();
    let mut sink = Sink::default();
    let mut refs = sources
        .iter_mut()
        .map(|s| s as &mut dyn CopySource)
        .collect::<Vec<_>>();
    let prepared = prepare(&job, &plan, &mut refs, &mut sink).unwrap();
    let files = sink
        .data
        .iter()
        .enumerate()
        .map(|(i, (name, bytes))| FetchFile {
            entry: file(name, bytes),
            volume_id: prepared.target.volume_id.clone(),
            file_id: format!("fixture-{i}"),
        })
        .collect::<Vec<_>>();
    let mut directories = std::collections::BTreeSet::new();
    for f in &files {
        for (i, _) in f.entry.path.as_str().match_indices('/') {
            directories.insert(RelativePath::new(&f.entry.path.as_str()[..i]).unwrap());
        }
    }
    let snapshot = FetchSnapshot {
        schema: "oev2.nx-fetch-snapshot/1".into(),
        binding: FetchBinding {
            root: job.root.clone(),
            target: prepared.target,
            job_id: job.job_id.clone(),
            manifest_digest: job.digest().unwrap(),
            job_record: BlobIdentity::of(&sink.data["job.json"]),
            state_record: None,
        },
        job,
        files,
        directories: directories.into_iter().collect(),
        qualification: "byte_observation_only".into(),
    };
    snapshot.validate().unwrap();
    snapshot
}
fn rebind_snapshot(snapshot: &mut oe_nx_protocol::large_fetch::FetchSnapshot) {
    let raw = serde_json::to_vec(&snapshot.job).unwrap();
    let identity = BlobIdentity::of(&raw);
    snapshot.binding.manifest_digest = snapshot.job.digest().unwrap();
    snapshot.binding.job_record = identity.clone();
    snapshot
        .files
        .iter_mut()
        .find(|f| f.entry.path.as_str() == "job.json")
        .unwrap()
        .entry
        .identity = identity;
}
#[test]
fn large_fetch_preserves_all_explicit_v3_auxiliary_records() {
    use oe_nx_protocol::large_fetch::*;
    let mut snapshot = byte_snapshot();
    if !snapshot
        .directories
        .iter()
        .any(|p| p.as_str() == "receipts")
    {
        snapshot
            .directories
            .push(RelativePath::new("receipts").unwrap());
    }
    for i in 0..(4555 + 18 + 20) {
        snapshot.files.push(FetchFile {
            entry: file(
                &format!("receipts/{i:04}-evidence.json"),
                b"complete original receipt",
            ),
            volume_id: "v".into(),
            file_id: format!("aux-{i}"),
        });
    }
    assert!(snapshot.validate().is_err());
    snapshot.job.auxiliary_capacity = Some(
        serde_json::from_value(
            json!({"max_total_bytes":536870912u64,"max_file_bytes":67108864u64,"max_files":8192}),
        )
        .unwrap(),
    );
    rebind_snapshot(&mut snapshot);
    snapshot.validate().unwrap();
    let groups = batches(&snapshot, MAX_PAYLOAD_BYTES).unwrap();
    let paths = groups
        .iter()
        .flat_map(|b| b.slices.iter().map(|s| s.file.entry.path.as_str()))
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(paths.len(), snapshot.files.len());
    assert_eq!(
        paths.iter().filter(|p| p.starts_with("receipts/")).count(),
        4555 + 18 + 20
    );
    let mut legacy = snapshot.clone();
    legacy.job.schema = JOB_SCHEMA.into();
    legacy.job.capacity = None;
    legacy.job.auxiliary_capacity = None;
    rebind_snapshot(&mut legacy);
    assert!(legacy.validate().is_err());
    let mut undeclared = snapshot.clone();
    undeclared.job.auxiliary_capacity = None;
    rebind_snapshot(&mut undeclared);
    assert!(undeclared.validate().is_err());
    let mut wrong = snapshot.clone();
    wrong.job.auxiliary_capacity.as_mut().unwrap().0.max_files = 4555;
    rebind_snapshot(&mut wrong);
    assert!(wrong.validate().is_err());
    let mut wrong = snapshot.clone();
    wrong
        .job
        .auxiliary_capacity
        .as_mut()
        .unwrap()
        .0
        .max_file_bytes = 1;
    rebind_snapshot(&mut wrong);
    assert!(wrong.validate().is_err());
    let mut wrong = snapshot.clone();
    let cap = &mut wrong.job.auxiliary_capacity.as_mut().unwrap().0;
    cap.max_total_bytes = 16384;
    cap.max_file_bytes = 16384;
    rebind_snapshot(&mut wrong);
    assert!(wrong.validate().is_err());
    let mut stale = snapshot.clone();
    stale.job.auxiliary_capacity.as_mut().unwrap().0.max_files += 1;
    assert!(stale.validate().is_err());
    let mut control = snapshot.clone();
    let f = control
        .files
        .iter_mut()
        .find(|f| f.entry.path.as_str() == "job.json")
        .unwrap();
    f.entry.identity.bytes = MAX_RECORD_BYTES as u64 + 1;
    control.binding.job_record = f.entry.identity.clone();
    assert!(control.validate().is_err());
}
#[test]
fn large_fetch_batches_bind_ranges_and_reject_false_complete_claims() {
    use oe_nx_protocol::large_fetch::*;
    let snapshot = byte_snapshot();
    let batches = batches(&snapshot, 20).unwrap();
    assert!(batches.len() > 1);
    let total = batches.iter().map(|b| b.validate().unwrap()).sum::<u64>();
    assert_eq!(
        total,
        snapshot
            .files
            .iter()
            .map(|f| f.entry.identity.bytes)
            .sum::<u64>()
    );
    let mut bad = batches[0].clone();
    bad.slices[0].bytes = MAX_PAYLOAD_BYTES + 1;
    assert!(bad.validate().is_err());
    let mut bad = batches[0].clone();
    bad.slices[0].offset = u64::MAX;
    assert!(bad.validate().is_err());
    let request = &batches[0];
    let body = vec![0; request.validate().unwrap() as usize];
    let mut response = FetchBatchResponse {
        schema: "oev2.nx-fetch-batch-response/1".into(),
        request_digest: record_digest(request).unwrap(),
        payload: BlobIdentity::of(&body),
        whole_file_hash_verified: false,
    };
    response.validate(request, &body).unwrap();
    response.whole_file_hash_verified = true;
    assert!(response.validate(request, &body).is_err());
    let mut bad = snapshot.clone();
    bad.files.push(bad.files[0].clone());
    assert!(bad.validate().is_err());
    let mut bad = snapshot;
    bad.directories.pop();
    assert!(bad.validate().is_err());
}
#[test]
fn large_fetch_snapshot_exceeds_record_limit_without_relaxing_frame_limits() {
    use oe_nx_protocol::large_fetch::*;
    let mut snapshot = byte_snapshot();
    snapshot.directories.push(RelativePath::new("out").unwrap());
    for i in 0..9000 {
        snapshot.files.push(FetchFile {
            entry: file(&format!("out/{}-{i:04}", "x".repeat(180)), b"x"),
            volume_id: "v".into(),
            file_id: format!("{}-{i}", "0".repeat(240)),
        });
    }
    snapshot
        .files
        .sort_by(|a, b| a.entry.path.cmp(&b.entry.path));
    snapshot.directories.sort();
    snapshot.validate().unwrap();
    let bytes = serde_json::to_vec(&snapshot).unwrap();
    assert!(bytes.len() > MAX_RECORD_BYTES);
    assert!(decode::<FetchSnapshot>(&bytes).is_err());
    assert_eq!(FetchSnapshot::decode(&bytes).unwrap(), snapshot);
    let response = FetchSnapshotResponse {
        schema: "oev2.nx-fetch-snapshot-response/1".into(),
        binding: snapshot.binding.clone(),
        snapshot_digest: snapshot.digest().unwrap(),
        payload: BlobIdentity::of(&bytes),
    };
    let frame = Frame {
        record: serde_json::to_value(response).unwrap(),
        payload: bytes,
    };
    let mut wire = Vec::new();
    write_frame(&mut wire, &frame).unwrap();
    let received = read_frame(&mut &wire[..]).unwrap();
    assert_eq!(
        FetchSnapshotResponse::decode_frame(
            received,
            &snapshot.binding.root,
            &snapshot.binding.job_id,
            &snapshot.binding.manifest_digest
        )
        .unwrap(),
        snapshot
    );
    assert!(batches(&snapshot, MAX_PAYLOAD_BYTES).unwrap().len() < 25);
}
