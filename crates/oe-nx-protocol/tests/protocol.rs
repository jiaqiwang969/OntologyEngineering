//! Pure wire/state fixtures only. No SDK, native result truth, geometry, license,
//! process survival or engineering acceptance is established by these tests.
use oe_nx_protocol::*;
use serde_json::{Value, json};
use std::io::{Cursor, Read};

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
fn controller() -> ProcessIdentity {
    ProcessIdentity {
        host_id: root().host_id,
        pid: 20,
        creation_time_100ns: 100,
        image: BlobIdentity::of(b"fixture controller"),
    }
}
fn process(job: &JobManifest) -> ProcessIdentity {
    ProcessIdentity {
        host_id: job.root.host_id.clone(),
        pid: 21,
        creation_time_100ns: 101,
        image: job.worker.executable.identity.clone(),
    }
}
fn intent(job: &JobManifest) -> DispatchIntent {
    DispatchIntent::new(
        job,
        controller(),
        Digest::of(b"fixture nonce not for execution"),
    )
    .unwrap()
}
fn staged(job: &JobManifest) -> StateRecord {
    let archive = BlobIdentity::of(b"fixture archive");
    let receipt = StageReceipt {
        schema: STAGE_SCHEMA.into(),
        job_id: job.job_id.clone(),
        root: job.root.clone(),
        manifest_digest: job.digest().unwrap(),
        payload_digest: inventory_digest(&job.payload_files()).unwrap(),
        archive: archive.clone(),
    };
    let mut state = StateRecord::prepared(job).unwrap();
    state.mark_staged(job, &receipt, &archive).unwrap();
    state
}
fn running(job: &JobManifest) -> StateRecord {
    let mut state = staged(job);
    state.begin_dispatch(job, intent(job)).unwrap();
    state.mark_running(job, process(job)).unwrap();
    state
}
fn result(job: &JobManifest) -> NativeResult {
    NativeResult {
        schema: RESULT_SCHEMA.into(),
        job_id: job.job_id.clone(),
        root: job.root.clone(),
        manifest_digest: job.digest().unwrap(),
        dispatch_id: intent(job).dispatch_id,
        worker_manifest_digest: job.worker.digest().unwrap(),
        worker: job.worker.executable.identity.clone(),
        input_digest: job.input_digest().unwrap(),
        process: process(job),
        runtime: runtime(),
        action: job.action.clone(),
        outcome: ResultOutcome::Succeeded,
        readback_schema: "fixture.readback/v1".into(),
        readback: json!({"scope":"wire-fixture-only"}),
        artifacts: vec![file("out/readback.json", b"fixture readback")],
        errors: vec![],
        unknowns: vec![],
    }
}
fn evidence(job: &JobManifest) -> CompletionEvidence {
    CompletionEvidence {
        process: process(job),
        exit_code: Some(0),
        runtime: runtime(),
        staged_payload: job.payload_files(),
        artifacts: result(job).artifacts,
    }
}

#[test]
fn strict_json_rejects_duplicates_at_every_level_without_private_marker_confusion() {
    for bytes in [
        br#"{"x":1,"x":2}"#.as_slice(),
        br#"{"x":{"a":1,"\u0061":2}}"#,
        br#"[{"k":0,"k":1}]"#,
        br#"{"x":1e999}"#,
        br#"{}{}"#,
    ] {
        assert!(json::strict_json(bytes).is_err());
    }
    let value=json::strict_json(br#"{"$serde_json::private::Number":"not a number","integer":123456789012345678901234567890}"#).unwrap();
    assert_eq!(value["$serde_json::private::Number"], "not a number");
    assert_eq!(
        value["integer"].to_string(),
        "123456789012345678901234567890"
    );
}

#[test]
fn direct_json_entry_retains_nx_record_budget_with_shared_parser() {
    let mut bytes = vec![b' '; MAX_RECORD_BYTES];
    bytes[..2].copy_from_slice(b"{}");
    assert_eq!(json::strict_json(&bytes).unwrap(), json!({}));
    bytes.push(b' ');
    assert!(json::strict_json(&bytes).is_err());
    assert!(decode::<Value>(&bytes).is_err());
}

#[test]
fn wire_types_refuse_unknown_fields_unknown_commands_and_legacy_execution() {
    let mut value = serde_json::to_value(job()).unwrap();
    value["legacy_journal"] = json!("worker.py");
    assert!(decode::<JobManifest>(&serde_json::to_vec(&value).unwrap()).is_err());
    for value in [
        json!({"command":"retry","job_id":"fixture-job"}),
        json!({"command":"probe","shell":"x"}),
        json!({"command":"run","job_id":"fixture-job","manifest_digest":Digest::of(b"x"),"wait_seconds":301}),
    ] {
        let parsed = decode::<Request>(&serde_json::to_vec(&value).unwrap());
        assert!(parsed.is_err() || parsed.unwrap().validate().is_err());
    }
    let mut legacy = job();
    legacy.schema = "cad-agent.nx-direct-job/v1".into();
    assert!(legacy.validate().is_err());
    assert!(decode::<Digest>(br#""ABCDEF""#).is_err());
}

#[test]
fn safe_unicode_paths_preserve_native_references_and_reject_windows_aliases() {
    let original = "inputs/子装配/滚珠丝杠.prt";
    assert_eq!(RelativePath::new(original).unwrap().as_str(), original);
    for path in [
        "/a",
        "../a",
        "a/../b",
        "a\\b",
        "a:b",
        "a/",
        "a//b",
        "aux.prt",
        "a/COM¹.txt",
        "LPT9.prt",
        "a.",
        "a ",
        "a\u{0}b",
        "a?b",
    ] {
        assert!(RelativePath::new(path).is_err(), "{path:?}");
    }
    for names in [
        ["inputs/A.prt", "inputs/a.prt"],
        ["inputs/é.prt", "inputs/e\u{301}.prt"],
        ["inputs/straße.prt", "inputs/STRASSE.prt"],
        ["inputs/ı.prt", "inputs/I.prt"],
        ["inputs/Ａ.prt", "inputs/A.prt"],
    ] {
        assert!(
            validate_inventory(&names.iter().map(|p| file(p, b"x")).collect::<Vec<_>>()).is_err(),
            "{names:?}"
        );
    }
    assert!(
        validate_inventory(&[file("inputs/轴", b"x"), file("inputs/轴/part.prt", b"y")]).is_err()
    );
    assert!(RelativePath::new(format!("inputs/{}.prt", "😀".repeat(100))).is_err());
}

#[test]
fn worker_capabilities_are_extensible_but_unknown_action_never_falls_through() {
    let mut job = job();
    job.action = "custom.thread_gauge".into();
    job.params_schema = "fixture.gauge/v1".into();
    assert!(job.validate().is_ok());
    job.action = "create_typo".into();
    assert!(job.validate().is_err());
    job.action = "inspect".into();
    assert!(job.validate().is_err());
    job.params_schema = "fixture.inspect/v1".into();
    job.worker
        .capabilities
        .push(job.worker.capabilities[0].clone());
    assert!(job.validate().is_err());
    let mut job = crate::job();
    job.worker.target = "x86_64-pc-windows-typo".into();
    assert!(job.validate().is_err());
}

#[test]
fn manifest_binds_sources_build_worker_parameters_and_every_input() {
    let job = job();
    job.validate().unwrap();
    job.verify_payload(&job.payload_files()).unwrap();
    let mut payload = job.payload_files();
    payload.reverse();
    job.verify_payload(&payload).unwrap();
    for index in 0..payload.len() {
        let mut changed = payload.clone();
        changed[index].identity = BlobIdentity::of(b"tampered");
        assert!(job.verify_payload(&changed).is_err());
        let mut missing = payload.clone();
        missing.remove(index);
        assert!(job.verify_payload(&missing).is_err());
    }
    payload.push(file("extra.py", b"undeclared"));
    assert!(job.verify_payload(&payload).is_err());
    let mut changed = job.clone();
    changed.worker.source_files[0].identity = BlobIdentity::of(b"other source");
    assert_ne!(job.digest().unwrap(), changed.digest().unwrap());
    let mut oversized = job.payload_files();
    oversized[0].identity.bytes = MAX_PAYLOAD_BYTES + 1;
    assert!(validate_inventory(&oversized).is_err());
}

#[test]
fn parameters_bind_original_bytes_and_duplicate_keys_are_not_normalized_away() {
    let job = job();
    let bytes = br#"{"schema":"fixture.inspect/v1","units":"mm"}"#;
    job.validate_parameters(bytes).unwrap();
    assert!(
        job.validate_parameters(br#"{"schema":"fixture.inspect/v1", "units":"mm"}"#)
            .is_err()
    );
    let duplicate = br#"{"schema":"fixture.inspect/v1","units":"mm","units":"inch"}"#;
    let mut job = job.clone();
    job.params = file("params.json", duplicate);
    assert!(job.validate_parameters(duplicate).is_err());
}

#[test]
fn staging_ack_requires_exact_archive_job_payload_and_stable_root() {
    let job = job();
    let archive = BlobIdentity::of(b"sent");
    let receipt = StageReceipt {
        schema: STAGE_SCHEMA.into(),
        job_id: job.job_id.clone(),
        root: root(),
        manifest_digest: job.digest().unwrap(),
        payload_digest: inventory_digest(&job.payload_files()).unwrap(),
        archive: archive.clone(),
    };
    receipt.validate(&job, &archive).unwrap();
    let value = serde_json::to_value(&receipt).unwrap();
    for (pointer, replacement) in [
        ("/job_id", json!("other-job")),
        ("/root/directory_id", json!("other-root")),
        ("/archive/bytes", json!(100)),
        ("/manifest_digest", json!(Digest::of(b"other"))),
        ("/payload_digest", json!(Digest::of(b"other"))),
    ] {
        let mut changed = value.clone();
        *changed.pointer_mut(pointer).unwrap() = replacement;
        let changed: StageReceipt = serde_json::from_value(changed).unwrap();
        assert!(changed.validate(&job, &archive).is_err());
    }
}

#[test]
fn one_dispatch_survives_serialization_and_unknown_cannot_retry_or_complete() {
    let job = job();
    let mut state = staged(&job);
    state.begin_dispatch(&job, intent(&job)).unwrap();
    let mut state: StateRecord = decode(&serde_json::to_vec(&state).unwrap()).unwrap();
    assert!(state.begin_dispatch(&job, intent(&job)).is_err());
    state.mark_unknown("connection_lost_after_intent").unwrap();
    assert!(state.begin_dispatch(&job, intent(&job)).is_err());
    assert!(state.mark_running(&job, process(&job)).is_err());
    assert!(state.finish(&job, &result(&job), &evidence(&job)).is_err());
    assert!(state.state.blocks_writes());
    state.state = State::Staged;
    assert!(state.validate(&job).is_err());
}

#[test]
fn known_completion_and_known_failure_are_terminal_single_use() {
    let job = job();
    let mut state = running(&job);
    state.finish(&job, &result(&job), &evidence(&job)).unwrap();
    assert_eq!(state.state, State::Completed);
    assert!(state.begin_dispatch(&job, intent(&job)).is_err());
    assert!(state.mark_unknown("rewrite").is_err());
    let mut failed = result(&job);
    failed.outcome = ResultOutcome::Failed;
    failed.errors.push("fixture_native_error".into());
    let mut state = running(&job);
    state.finish(&job, &failed, &evidence(&job)).unwrap();
    assert_eq!(state.state, State::Failed);
    assert!(!state.state.blocks_writes());
    assert!(state.begin_dispatch(&job, intent(&job)).is_err());
}

#[test]
fn stale_receipts_or_any_job_worker_process_runtime_input_artifact_mismatch_hold() {
    let job = job();
    let baseline = serde_json::to_value(result(&job)).unwrap();
    let variants: Vec<(&str, Value)> = vec![
        ("/job_id", json!("other-job")),
        ("/root/volume_id", json!("other-volume")),
        ("/manifest_digest", json!(Digest::of(b"other"))),
        ("/dispatch_id", json!(Digest::of(b"previous dispatch"))),
        (
            "/worker_manifest_digest",
            json!(Digest::of(b"other source")),
        ),
        ("/worker/sha256", json!(Digest::of(b"other EXE"))),
        ("/input_digest", json!(Digest::of(b"other input"))),
        ("/process/pid", json!(30)),
        ("/process/creation_time_100ns", json!(999)),
        ("/process/host_id", json!("other-host")),
        (
            "/runtime/libraries/0/identity/sha256",
            json!(Digest::of(b"other DLL")),
        ),
        ("/action", json!("create")),
        ("/artifacts/0/identity/bytes", json!(99)),
        ("/artifacts/0/path", json!("out/stale.prt")),
    ];
    for (pointer, replacement) in variants {
        let mut value = baseline.clone();
        *value.pointer_mut(pointer).unwrap() = replacement;
        let changed: NativeResult = serde_json::from_value(value).unwrap();
        let mut state = running(&job);
        state.finish(&job, &changed, &evidence(&job)).unwrap();
        assert_eq!(state.state, State::UnknownAfterDispatch, "{pointer}");
    }
}

#[test]
fn worker_success_cannot_override_unknown_exit_wrong_observations_or_unknowns() {
    let job = job();
    for mode in 0..8 {
        let mut result = result(&job);
        let mut observed = evidence(&job);
        match mode {
            0 => observed.exit_code = None,
            1 => observed.exit_code = Some(7),
            2 => observed.process.creation_time_100ns += 1,
            3 => observed.staged_payload[0].identity = BlobIdentity::of(b"worker modified midrun"),
            4 => observed
                .artifacts
                .push(file("out/unreported.prt", b"extra")),
            5 => result.unknowns.push("units_unobserved".into()),
            6 => result.outcome = ResultOutcome::Unknown,
            7 => observed.runtime.product_label = "unbound".into(),
            _ => unreachable!(),
        }
        let mut state = running(&job);
        state.finish(&job, &result, &observed).unwrap();
        assert_eq!(state.state, State::UnknownAfterDispatch, "mode {mode}");
        assert!(state.result_digest.is_some());
    }
}

#[test]
fn root_interlock_covers_pending_missing_corrupt_legacy_and_unknown_states() {
    for input in [
        HoldInput::Missing,
        HoldInput::PendingWrite,
        HoldInput::IoUncertain("reparse".into()),
        HoldInput::StateBytes(b"{".to_vec()),
        HoldInput::StateBytes(br#"{"schema":"future","state":"COMPLETED"}"#.to_vec()),
    ] {
        assert!(classify_hold(&input).blocks_writes);
    }
    for state in [
        "STARTING",
        "RUNNING",
        "UNKNOWN_AFTER_DISPATCH",
        "UNKNOWN",
        "invented",
    ] {
        let input=HoldInput::StateBytes(serde_json::to_vec(&json!({"schema":LEGACY_STATE_SCHEMA,"job_id":"old-job","state":state,"hostname":"legacy-host","extra_old_field":42})).unwrap());
        let hold = classify_hold(&input);
        assert!(hold.blocks_writes);
        assert!(hold.legacy_v1);
        assert_eq!(hold.observed_record.unwrap()["extra_old_field"], 42);
    }
    let job = job();
    let clear = classify_hold(&HoldInput::StateBytes(
        serde_json::to_vec(&staged(&job)).unwrap(),
    ));
    assert!(!clear.blocks_writes);
    assert!(assess_root_holds(&[
        clear,
        classify_hold(&HoldInput::PendingWrite)
    ]));
}

#[test]
fn scan_binds_terminal_record_to_directory_and_pinned_root() {
    let job = job();
    let mut state = running(&job);
    state.finish(&job, &result(&job), &evidence(&job)).unwrap();
    let input = HoldInput::StateBytes(serde_json::to_vec(&state).unwrap());
    assert!(!classify_job_hold(&input, &job.job_id, &job.root).blocks_writes);
    assert!(classify_job_hold(&input, &JobId::new("other-job").unwrap(), &job.root).blocks_writes);
    let mut different = root();
    different.directory_id = "other".into();
    assert!(classify_job_hold(&input, &job.job_id, &different).blocks_writes);
    let mut missing_identity = serde_json::to_value(state).unwrap();
    missing_identity["process"] = Value::Null;
    assert!(
        classify_hold(&HoldInput::StateBytes(
            serde_json::to_vec(&missing_identity).unwrap()
        ))
        .blocks_writes
    );
}

#[test]
fn closed_request_status_and_fetch_preserve_legacy_ids_without_run_authority() {
    for verb in ["status", "fetch"] {
        let request: Request =
            decode(&serde_json::to_vec(&json!({"command":verb,"job_id":"old-job"})).unwrap())
                .unwrap();
        request.validate().unwrap();
    }
    assert!(JobId::new("NUL").is_err());
    assert!(JobId::new("../escape").is_err());
    let request:Request=decode(&serde_json::to_vec(&json!({"command":"run","job_id":"old-job","manifest_digest":Digest::of(b"anything"),"wait_seconds":1})).unwrap()).unwrap();
    request.validate().unwrap(); // Binding its digest to a v2 JobManifest is still mandatory at dispatch.
    let mut old = job();
    old.schema = "cad-agent.nx-direct-job/v1".into();
    assert!(StateRecord::prepared(&old).is_err());
}

#[test]
fn same_dispatch_reconciliation_closes_only_proven_outcome_and_preserves_unknown_history() {
    let job = job();
    let mut state = running(&job);
    state.mark_unknown("ssh_connection_lost").unwrap();
    let history = state.uncertainty_history.clone();
    let mut changed = evidence(&job);
    changed.process.creation_time_100ns += 1;
    assert!(!state.reconcile(&job, &result(&job), &changed).unwrap());
    assert_eq!(state.state, State::UnknownAfterDispatch);
    changed = evidence(&job);
    changed.exit_code = None;
    assert!(!state.reconcile(&job, &result(&job), &changed).unwrap());
    let mut stale = result(&job);
    stale.dispatch_id = Digest::of(b"previous");
    assert!(!state.reconcile(&job, &stale, &evidence(&job)).unwrap());
    assert!(
        state
            .reconcile(&job, &result(&job), &evidence(&job))
            .unwrap()
    );
    assert_eq!(state.state, State::Completed);
    assert_eq!(state.uncertainty_history, history);
    assert!(state.begin_dispatch(&job, intent(&job)).is_err());
    let roundtrip: StateRecord = decode(&serde_json::to_vec(&state).unwrap()).unwrap();
    roundtrip.validate(&job).unwrap();
    assert_eq!(roundtrip.uncertainty_history, history);
    let mut unknown_start = staged(&job);
    unknown_start.begin_dispatch(&job, intent(&job)).unwrap();
    unknown_start
        .mark_unknown("lost_before_process_identity")
        .unwrap();
    assert!(
        !unknown_start
            .reconcile(&job, &result(&job), &evidence(&job))
            .unwrap()
    );
    let mut failed = result(&job);
    failed.outcome = ResultOutcome::Failed;
    failed.errors.push("known_native_error".into());
    let mut state = running(&job);
    state.mark_unknown("wait_interrupted").unwrap();
    assert!(state.reconcile(&job, &failed, &evidence(&job)).unwrap());
    assert_eq!(state.state, State::Failed);
    assert_eq!(state.uncertainty_history.len(), 1);
}

#[test]
fn legacy_failed_or_completed_string_never_clears_hold_without_bound_termination() {
    let job_id = JobId::new("old-job").unwrap();
    let journal = Digest::of(b"old journal");
    let state = json!({"schema":LEGACY_STATE_SCHEMA,"job_id":"old-job","state":"FAILED","hostname":"legacy-host","native_pid":50,
        "native_start_utc":"2023-11-14T22:13:20.0000000Z","journal_sha256":journal,"exit_code":1,"old_extra":"preserved"});
    let input = HoldInput::StateBytes(serde_json::to_vec(&state).unwrap());
    let result = json!({"schema":"cad-agent.nx-direct-result/v1","job_id":"old-job","pid":50,"journal_sha256":journal,
        "started_at":1700000000.01,"finished_at":1700000001.0,"status":"error","error":"fixture native error","outputs":[]});
    let mut evidence = LegacyCompletionEvidence {
        native_pid: 50,
        creation_time_100ns: 133_444_736_000_000_000,
        native_start_utc: "2023-11-14T22:13:20.0000000Z".into(),
        hostname: "legacy-host".into(),
        exit_code: Some(1),
        journal_sha256: journal,
        native_result: serde_json::to_vec(&result).unwrap(),
    };
    assert!(classify_job_hold(&input, &job_id, &root()).blocks_writes);
    let clear = classify_legacy_completion(&input, &job_id, &root(), &evidence);
    assert!(!clear.blocks_writes);
    assert_eq!(clear.observed_record.unwrap()["old_extra"], "preserved");
    let original = evidence.clone();
    evidence.native_result = b"{}".to_vec();
    assert!(classify_legacy_completion(&input, &job_id, &root(), &evidence).blocks_writes);
    evidence = original.clone();
    evidence.exit_code = None;
    assert!(classify_legacy_completion(&input, &job_id, &root(), &evidence).blocks_writes);
    evidence = original.clone();
    evidence.creation_time_100ns += 100_000_000;
    assert!(classify_legacy_completion(&input, &job_id, &root(), &evidence).blocks_writes);
    evidence = original.clone();
    evidence.native_pid = 51;
    assert!(classify_legacy_completion(&input, &job_id, &root(), &evidence).blocks_writes);
    evidence = original.clone();
    evidence.journal_sha256 = Digest::of(b"other");
    assert!(classify_legacy_completion(&input, &job_id, &root(), &evidence).blocks_writes);
    let mut complete = state.clone();
    complete["state"] = json!("COMPLETED");
    assert!(
        classify_hold(&HoldInput::StateBytes(
            serde_json::to_vec(&complete).unwrap()
        ))
        .blocks_writes
    );
    assert!(
        classify_legacy_completion(&HoldInput::PendingWrite, &job_id, &root(), &original)
            .blocks_writes
    );
}

#[test]
fn historical_probe_reconciliation_requires_control_plane_digest_and_all_original_bytes() {
    let job_id = JobId::new("historical-probe").unwrap();
    let state_bytes=serde_json::to_vec(&json!({"schema":LEGACY_STATE_SCHEMA,"job_id":job_id,"state":"COMPLETED","hostname":"legacy-host",
        "native_pid":55,"native_start_utc":"2026-09-28T10:00:00Z","exit_code":0,"scope":"experimental_controller_not_product_migration"})).unwrap();
    let input = HoldInput::StateBytes(state_bytes.clone());
    let references = vec![
        file("evidence/source.rs", b"original source"),
        file("evidence/build.json", b"binary source binding"),
        file("evidence/native.json", b"native event receipt"),
        file(
            "evidence/run.json",
            b"process exit and GUI unchanged receipt",
        ),
    ];
    let review = LegacyReconciliationReview {
        schema: LEGACY_REVIEW_SCHEMA.into(),
        scope: LegacyReviewScope::WriterHoldReleaseOnly,
        job_id: job_id.clone(),
        root: root(),
        original_state: BlobIdentity::of(&state_bytes),
        process: LegacyProcessTermination {
            hostname: "legacy-host".into(),
            native_pid: 55,
            native_start_utc: "2026-09-28T10:00:00Z".into(),
            exit_code: 0,
        },
        references: references.clone(),
        termination_record: RelativePath::new("evidence/run.json").unwrap(),
        source_records: vec![
            RelativePath::new("evidence/source.rs").unwrap(),
            RelativePath::new("evidence/build.json").unwrap(),
        ],
        native_records: vec![RelativePath::new("evidence/native.json").unwrap()],
        reviewer: "authorized-migration-reviewer".into(),
        reviewed_at_utc: "2026-09-28T11:00:00Z".into(),
        reason: "fixture historical termination evidence only".into(),
    };
    let bytes = serde_json::to_vec(&review).unwrap();
    let pinned = Digest::of(&bytes);
    let result =
        classify_reviewed_legacy(&input, &job_id, &root(), &bytes, &pinned, &references).unwrap();
    assert!(!result.blocks_writes);
    assert!(result.legacy_v1);
    assert_eq!(
        result.observed_record.unwrap()["scope"],
        "experimental_controller_not_product_migration"
    );
    assert!(
        classify_reviewed_legacy(
            &input,
            &job_id,
            &root(),
            &bytes,
            &Digest::of(b"other review"),
            &references
        )
        .is_err()
    );
    for index in 0..references.len() {
        let mut changed = references.clone();
        changed[index].identity = BlobIdentity::of(b"changed evidence");
        assert!(
            classify_reviewed_legacy(&input, &job_id, &root(), &bytes, &pinned, &changed).is_err()
        );
    }
    let mut changed = state_bytes.clone();
    changed.push(b' ');
    assert!(
        classify_reviewed_legacy(
            &HoldInput::StateBytes(changed),
            &job_id,
            &root(),
            &bytes,
            &pinned,
            &references
        )
        .is_err()
    );
    let mut other_root = root();
    other_root.directory_id = "alias-to-different-root".into();
    assert!(
        classify_reviewed_legacy(&input, &job_id, &other_root, &bytes, &pinned, &references)
            .is_err()
    );
    assert!(
        classify_reviewed_legacy(
            &HoldInput::PendingWrite,
            &job_id,
            &root(),
            &bytes,
            &pinned,
            &references
        )
        .is_err()
    );
    let mut self_approval = serde_json::to_value(review).unwrap();
    self_approval["scope"] = json!("engineering_pass");
    let bytes = serde_json::to_vec(&self_approval).unwrap();
    assert!(decode::<LegacyReconciliationReview>(&bytes).is_err());
}

fn raw_frame(record: &[u8], payload: &[u8]) -> Vec<u8> {
    let mut bytes = FRAME_MAGIC.to_vec();
    bytes.extend((record.len() as u32).to_le_bytes());
    bytes.extend((payload.len() as u64).to_le_bytes());
    bytes.extend(record);
    bytes.extend(payload);
    bytes
}
struct ShortReader<'a> {
    bytes: &'a [u8],
    position: usize,
    max_chunk: usize,
}
impl Read for ShortReader<'_> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        // Deliberately error if the implementation waits for EOF after consuming
        // the complete frame, as Windows stdin may remain open indefinitely.
        if self.position == self.bytes.len() {
            return Err(std::io::Error::other("unexpected EOF wait"));
        }
        let count = out
            .len()
            .min(self.max_chunk)
            .min(self.bytes.len() - self.position);
        out[..count].copy_from_slice(&self.bytes[self.position..self.position + count]);
        self.position += count;
        Ok(count)
    }
}

#[test]
fn framing_handles_real_short_reads_and_returns_without_waiting_for_eof() {
    let request = Request::Stage {
        job: Box::new(job()),
        archive: BlobIdentity::of(b"bounded zip fixture"),
    };
    let mut bytes = vec![];
    write_request(&mut bytes, &request, b"bounded zip fixture").unwrap();
    let mut reader = ShortReader {
        bytes: &bytes,
        position: 0,
        max_chunk: 1,
    };
    let (decoded, payload) = read_request(&mut reader).unwrap();
    assert_eq!(decoded, request);
    assert_eq!(payload, b"bounded zip fixture");
    assert_eq!(reader.position, bytes.len());
    let one = raw_frame(br#"{"command":"probe"}"#, b"");
    let mut two = one.clone();
    two.extend(b"unrelated next bytes");
    let mut cursor = Cursor::new(two);
    assert_eq!(read_request(&mut cursor).unwrap().0, Request::Probe {});
    assert_eq!(cursor.position(), one.len() as u64);
}

#[test]
fn framing_refuses_every_truncation_and_rejects_oversize_before_body_read() {
    let bytes = raw_frame(br#"{"command":"probe"}"#, b"");
    for length in 0..bytes.len() {
        assert!(
            read_request(&mut Cursor::new(&bytes[..length])).is_err(),
            "cut {length}"
        );
    }
    for (json_len, payload_len) in [
        (0u32, 0u64),
        ((MAX_RECORD_BYTES + 1) as u32, 0),
        (2, MAX_PAYLOAD_BYTES + 1),
        (u32::MAX, u64::MAX),
    ] {
        let mut header = FRAME_MAGIC.to_vec();
        header.extend(json_len.to_le_bytes());
        header.extend(payload_len.to_le_bytes());
        let mut reader = ShortReader {
            bytes: &header,
            position: 0,
            max_chunk: 3,
        };
        let error = read_request(&mut reader).unwrap_err();
        assert!(error.to_string().contains("bound"));
        assert_eq!(reader.position, FRAME_HEADER_BYTES);
    }
    let mut wrong = bytes.clone();
    wrong[0] = b'?';
    assert!(read_request(&mut Cursor::new(wrong)).is_err());
    let duplicate = raw_frame(br#"{"command":"probe","command":"probe"}"#, b"");
    assert!(read_request(&mut Cursor::new(duplicate)).is_err());
}

#[test]
fn framing_rejects_nonstage_payload_before_read_and_binds_stage_archive_bytes() {
    let mut bytes = raw_frame(br#"{"command":"probe"}"#, b"");
    bytes[12..20].copy_from_slice(&100u64.to_le_bytes());
    let mut reader = ShortReader {
        bytes: &bytes,
        position: 0,
        max_chunk: 7,
    };
    assert!(
        read_request(&mut reader)
            .unwrap_err()
            .to_string()
            .contains("only stage")
    );
    assert_eq!(reader.position, bytes.len());
    let request = Request::Stage {
        job: Box::new(job()),
        archive: BlobIdentity::of(b"abc"),
    };
    let json = serde_json::to_vec(&request).unwrap();
    assert!(read_request(&mut Cursor::new(raw_frame(&json, b"abd"))).is_err());
    assert!(read_request(&mut Cursor::new(raw_frame(&json, b"ab"))).is_err());
    let valid = raw_frame(&json, b"abc");
    assert!(read_request(&mut Cursor::new(&valid[..valid.len() - 1])).is_err());
    assert!(write_request(&mut vec![], &Request::Probe {}, b"unexpected").is_err());
}

#[test]
fn response_frame_fetch_payload_requires_exact_typed_receipt_identity() {
    let frame = Frame {
        record: json!({"status":"fetched","archive":BlobIdentity::of(b"zip fixture")}),
        payload: b"zip fixture".to_vec(),
    };
    let mut bytes = vec![];
    write_frame(&mut bytes, &frame).unwrap();
    let decoded = read_frame(&mut Cursor::new(bytes)).unwrap();
    assert_eq!(decoded, frame);
    let receipt: BlobIdentity = serde_json::from_value(decoded.record["archive"].clone()).unwrap();
    decoded.validate_payload(Some(&receipt)).unwrap();
    assert!(decoded.validate_payload(None).is_err());
    assert!(
        decoded
            .validate_payload(Some(&BlobIdentity::of(b"different")))
            .is_err()
    );
}

#[test]
fn typed_probe_binds_profile_controller_and_root_without_native_execution_claims() {
    let mut reply = ProbeResponse {
        schema: PROBE_RESPONSE_SCHEMA.into(),
        root: root(),
        controller: controller(),
        native_execution_started: false,
        stage_run_requires_expected_root: true,
    };
    reply.validate_binding(None, &controller().image).unwrap();
    reply
        .validate_binding(Some(&root()), &controller().image)
        .unwrap();
    reply.native_execution_started = true;
    assert!(reply.validate_binding(None, &controller().image).is_err());
    reply.native_execution_started = false;
    reply.stage_run_requires_expected_root = false;
    assert!(reply.validate_binding(None, &controller().image).is_err());
    reply.stage_run_requires_expected_root = true;
    assert!(
        reply
            .validate_binding(None, &BlobIdentity::of(b"other controller"))
            .is_err()
    );
    let mut changed = root();
    changed.directory_id = "other root".into();
    assert!(
        reply
            .validate_binding(Some(&changed), &controller().image)
            .is_err()
    );
}

#[test]
fn typed_stage_reply_binds_unused_state_and_actual_state_file_bytes() {
    let job = job();
    let archive = BlobIdentity::of(b"sent archive");
    let state = staged(&job);
    let raw = serde_json::to_vec_pretty(&state).unwrap();
    let mut reply = StageResponse {
        schema: STAGE_RESPONSE_SCHEMA.into(),
        receipt: StageReceipt {
            schema: STAGE_SCHEMA.into(),
            job_id: job.job_id.clone(),
            root: root(),
            manifest_digest: job.digest().unwrap(),
            payload_digest: inventory_digest(&job.payload_files()).unwrap(),
            archive: archive.clone(),
        },
        state,
        state_file: BlobIdentity::of(&raw),
    };
    reply.validate_binding(&job, &archive).unwrap();
    reply.verify_state_bytes(&raw).unwrap();
    assert!(
        reply
            .verify_state_bytes(&serde_json::to_vec(&reply.state).unwrap())
            .is_err()
    );
    reply.state.begin_dispatch(&job, intent(&job)).unwrap();
    assert!(reply.validate_binding(&job, &archive).is_err());
}

#[test]
fn typed_status_preserves_unknown_and_verifies_raw_bytes_and_original_run_manifest() {
    let job = job();
    let mut state = running(&job);
    state.mark_unknown("connection_lost").unwrap();
    let raw = serde_json::to_vec_pretty(&state).unwrap();
    let assessment = classify_job_hold(&HoldInput::StateBytes(raw.clone()), &job.job_id, &job.root);
    let mut reply = StatusResponse {
        schema: STATUS_RESPONSE_SCHEMA.into(),
        job_id: job.job_id.clone(),
        root: root(),
        state: serde_json::to_value(state).unwrap(),
        state_file: BlobIdentity::of(&raw),
        blocks_writes: assessment.blocks_writes,
        hold_reason: assessment.reason.clone(),
        legacy_read_only: false,
        automatic_retry: false,
    };
    reply.validate_binding(&job.job_id, &job.root).unwrap();
    reply.validate_job(&job).unwrap();
    reply.verify_state_bytes(&raw).unwrap();
    reply.validate_hold(&assessment).unwrap();
    reply.blocks_writes = false;
    assert!(reply.validate_binding(&job.job_id, &job.root).is_err());
    reply.blocks_writes = true;
    reply.automatic_retry = true;
    assert!(reply.validate_binding(&job.job_id, &job.root).is_err());
    reply.automatic_retry = false;
    reply.state["manifest_digest"] = serde_json::to_value(Digest::of(b"other manifest")).unwrap();
    assert!(reply.validate_job(&job).is_err());
    assert!(reply.verify_state_bytes(&raw).is_err());
}

#[test]
fn legacy_status_metadata_never_substitutes_for_validated_hold_release() {
    let job = job();
    let state = serde_json::json!({"schema":LEGACY_STATE_SCHEMA,"job_id":job.job_id,"state":"COMPLETED","old_extra":"preserved"});
    let raw = serde_json::to_vec_pretty(&state).unwrap();
    let assessment = classify_job_hold(&HoldInput::StateBytes(raw.clone()), &job.job_id, &job.root);
    let mut reply = StatusResponse {
        schema: STATUS_RESPONSE_SCHEMA.into(),
        job_id: job.job_id.clone(),
        root: root(),
        state,
        state_file: BlobIdentity::of(&raw),
        blocks_writes: true,
        hold_reason: assessment.reason.clone(),
        legacy_read_only: true,
        automatic_retry: false,
    };
    reply.validate_binding(&job.job_id, &job.root).unwrap();
    reply.verify_state_bytes(&raw).unwrap();
    reply.validate_hold(&assessment).unwrap();
    assert!(reply.validate_job(&job).is_err());
    reply.blocks_writes = false;
    reply.hold_reason = "peer_only_claimed_release".into();
    reply.validate_binding(&job.job_id, &job.root).unwrap(); // identity only, explicitly separate from release authority
    assert!(reply.validate_hold(&assessment).is_err());
    reply.legacy_read_only = false;
    assert!(reply.validate_binding(&job.job_id, &job.root).is_err());
}

#[test]
fn typed_fetch_binds_zip_every_member_and_original_state_including_recovery_bytes() {
    let job = job();
    let mut members = std::collections::BTreeMap::new();
    members.insert(
        "state.json".into(),
        b"{malformed old state preserved for inspection".to_vec(),
    );
    members.insert("out/part.prt".into(), b"fixture data".to_vec());
    let bytes = bundle::archive_members(&members).unwrap();
    let mut reply = FetchResponse {
        schema: FETCH_RESPONSE_SCHEMA.into(),
        job_id: job.job_id.clone(),
        root: root(),
        archive: BlobIdentity::of(&bytes),
        members: members
            .iter()
            .map(|(name, bytes)| file(name, bytes))
            .collect(),
        state_file: Some(BlobIdentity::of(&members["state.json"])),
    };
    assert_eq!(
        reply
            .validate_binding(&job.job_id, &job.root, &bytes)
            .unwrap(),
        members
    );
    assert!(
        reply
            .validate_binding(&JobId::new("other-job").unwrap(), &job.root, &bytes)
            .is_err()
    );
    let original = reply.clone();
    reply.members.pop();
    assert!(
        reply
            .validate_binding(&job.job_id, &job.root, &bytes)
            .is_err()
    );
    reply = original.clone();
    reply.state_file = Some(BlobIdentity::of(b"other raw state"));
    assert!(
        reply
            .validate_binding(&job.job_id, &job.root, &bytes)
            .is_err()
    );
    reply = original;
    let mut changed = bytes.clone();
    changed[0] ^= 1;
    assert!(
        reply
            .validate_binding(&job.job_id, &job.root, &changed)
            .is_err()
    );
    members.insert("state.json".into(), Vec::new());
    let empty_archive = bundle::archive_members(&members).unwrap();
    reply.archive = BlobIdentity::of(&empty_archive);
    reply.state_file = Some(BlobIdentity::of(b""));
    reply.members = members
        .iter()
        .map(|(name, bytes)| file(name, bytes))
        .collect();
    assert_eq!(
        reply
            .validate_binding(&job.job_id, &job.root, &empty_archive)
            .unwrap()["state.json"],
        b""
    );
    reply.state_file = None;
    assert!(
        reply
            .validate_binding(&job.job_id, &job.root, &empty_archive)
            .is_err()
    );
    members.remove("state.json");
    let no_state_archive = bundle::archive_members(&members).unwrap();
    reply.archive = BlobIdentity::of(&no_state_archive);
    reply.members = members
        .iter()
        .map(|(name, bytes)| file(name, bytes))
        .collect();
    assert!(
        !reply
            .validate_binding(&job.job_id, &job.root, &no_state_archive)
            .unwrap()
            .contains_key("state.json")
    );
    reply.state_file = Some(BlobIdentity::of(b""));
    assert!(
        reply
            .validate_binding(&job.job_id, &job.root, &no_state_archive)
            .is_err()
    );
}

#[test]
fn typed_error_is_closed_and_never_authorizes_retry() {
    let mut reply = ErrorResponse {
        schema: ERROR_RESPONSE_SCHEMA.into(),
        error: "blocked_by_known_root_hold".into(),
        automatic_retry: false,
    };
    reply.validate().unwrap();
    reply.automatic_retry = true;
    assert!(reply.validate().is_err());
    assert!(decode::<ErrorResponse>(br#"{"schema":"cad-agent.nx-error/v2","error":"x","automatic_retry":false,"retry_delay":2}"#).is_err());
}

#[test]
fn unknown_status_preserves_malformed_empty_and_future_state_bytes_as_a_hold() {
    let job = job();
    for raw in [
        b"".as_slice(),
        b"{broken JSON",
        br#"{"schema":"future/v99","state":"COMPLETE"}"#,
    ] {
        let reply = UnknownStatusResponse {
            schema: UNKNOWN_STATUS_RESPONSE_SCHEMA.into(),
            job_id: job.job_id.clone(),
            root: root(),
            state_file: Some(BlobIdentity::of(raw)),
            blocks_writes: true,
            reason: "state_not_interpretable".into(),
            automatic_retry: false,
        };
        reply.validate_binding(&job.job_id, &job.root).unwrap();
        reply.verify_state_bytes(raw).unwrap();
        assert!(reply.verify_state_bytes(b"different bytes").is_err());
    }
}

#[test]
fn unknown_status_missing_state_cannot_drop_hold_retry_or_claim_observed_bytes() {
    let job = job();
    let mut reply = UnknownStatusResponse {
        schema: UNKNOWN_STATUS_RESPONSE_SCHEMA.into(),
        job_id: job.job_id.clone(),
        root: root(),
        state_file: None,
        blocks_writes: true,
        reason: "missing_or_unreadable_state".into(),
        automatic_retry: false,
    };
    reply.validate_binding(&job.job_id, &job.root).unwrap();
    assert!(reply.verify_state_bytes(b"").is_err());
    reply.blocks_writes = false;
    assert!(reply.validate_binding(&job.job_id, &job.root).is_err());
    reply.blocks_writes = true;
    reply.automatic_retry = true;
    assert!(reply.validate_binding(&job.job_id, &job.root).is_err());
    reply.automatic_retry = false;
    let mut changed = root();
    changed.directory_id = "different-directory".into();
    assert!(reply.validate_binding(&job.job_id, &changed).is_err());
    let mut value = serde_json::to_value(reply).unwrap();
    value["native_acceptance"] = json!("passed");
    assert!(decode::<UnknownStatusResponse>(&serde_json::to_vec(&value).unwrap()).is_err());
}

#[test]
fn shared_status_observation_dispatches_schema_and_never_validates_unknown_as_a_job() {
    let job = job();
    let unknown = UnknownStatusResponse {
        schema: UNKNOWN_STATUS_RESPONSE_SCHEMA.into(),
        job_id: job.job_id.clone(),
        root: root(),
        state_file: None,
        blocks_writes: true,
        reason: "state_missing".into(),
        automatic_retry: false,
    };
    let raw = serde_json::to_vec(&unknown).unwrap();
    let observation = StatusObservation::decode(&raw).unwrap();
    assert!(matches!(observation, StatusObservation::Unknown(_)));
    observation
        .validate_binding(&job.job_id, &job.root)
        .unwrap();
    assert!(observation.validate_job(&job).is_err());
    assert_eq!(
        serde_json::to_value(&observation).unwrap(),
        serde_json::to_value(unknown).unwrap()
    );
    let state = staged(&job);
    let bytes = serde_json::to_vec(&state).unwrap();
    let hold = classify_job_hold(
        &HoldInput::StateBytes(bytes.clone()),
        &job.job_id,
        &job.root,
    );
    let known = StatusResponse {
        schema: STATUS_RESPONSE_SCHEMA.into(),
        job_id: job.job_id.clone(),
        root: root(),
        state: serde_json::to_value(state).unwrap(),
        state_file: BlobIdentity::of(&bytes),
        blocks_writes: hold.blocks_writes,
        hold_reason: hold.reason,
        legacy_read_only: false,
        automatic_retry: false,
    };
    let observation = StatusObservation::from_value(serde_json::to_value(known).unwrap()).unwrap();
    assert!(matches!(observation, StatusObservation::Known(_)));
    observation.validate_job(&job).unwrap();
    assert!(StatusObservation::decode(br#"{"schema":"cad-agent.nx-status-unknown/v2","schema":"cad-agent.nx-status-observation/v2"}"#).is_err());
    assert!(StatusObservation::from_value(json!({"schema":"future/v3"})).is_err());
}

#[test]
fn recorded_receiving_does_not_replace_live_runtime_or_process_observations() {
    let job = job();
    let result = result(&job);
    let dispatch = intent(&job);
    let process = process(&job);
    result
        .validate_recorded_binding(
            &job,
            &dispatch,
            &process,
            &job.payload_files(),
            &result.artifacts,
        )
        .unwrap();
    let mut observed = evidence(&job);
    observed.runtime.libraries[0].identity = BlobIdentity::of(b"different loaded runtime");
    assert!(
        result
            .validate_binding(&job, &dispatch, &process, &observed)
            .is_err()
    );
    observed = evidence(&job);
    observed.process.creation_time_100ns += 1;
    assert!(
        result
            .validate_binding(&job, &dispatch, &process, &observed)
            .is_err()
    );
}
#[test]
fn recorded_receiving_retains_dispatch_inventory_and_unresolved_success_guards() {
    let job = job();
    let result = result(&job);
    let dispatch = intent(&job);
    let process = process(&job);
    let check = |r: &NativeResult, payload: &[FileEntry], outputs: &[FileEntry]| {
        r.validate_recorded_binding(&job, &dispatch, &process, payload, outputs)
    };
    let mut changed = result.clone();
    changed.dispatch_id = Digest::of(b"different dispatch");
    assert!(check(&changed, &job.payload_files(), &result.artifacts).is_err());
    assert!(check(&result, &[], &result.artifacts).is_err());
    let outputs = vec![file("out/readback.json", b"changed output")];
    assert!(check(&result, &job.payload_files(), &outputs).is_err());
    changed = result.clone();
    changed.unknowns.push("native outcome unknown".into());
    assert!(check(&changed, &job.payload_files(), &result.artifacts).is_err());
}
