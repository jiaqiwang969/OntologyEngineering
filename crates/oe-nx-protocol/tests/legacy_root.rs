//! Pure bookkeeping counterexamples. No live process or old CAD is accepted.
use oe_nx_protocol::*;
use serde_json::json;

fn file(name: &str, bytes: &[u8]) -> FileEntry {
    FileEntry::from_bytes(name, bytes).unwrap()
}
fn root() -> RootIdentity {
    RootIdentity {
        host_id: "reviewed-host".into(),
        volume_id: "volume".into(),
        directory_id: "root-id".into(),
    }
}
fn directory() -> RootIdentity {
    RootIdentity {
        directory_id: "job-id".into(),
        ..root()
    }
}
fn process() -> LegacyRecordedProcess {
    LegacyRecordedProcess {
        hostname: "OLD-WINDOWS".into(),
        pid: 42,
        creation_time_100ns: 133_444_736_000_000_000,
        native_start_utc: "2023-11-14T22:13:20.0000000Z".into(),
    }
}
fn state(schema: &str, status: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({"schema":schema,"state":status,"job_id":"old-job","hostname":process().hostname,"native_pid":42,"native_start_utc":process().native_start_utc,"exit_code":-1,"old_original_evidence":"unchanged"})).unwrap()
}
fn terminal(bytes: &[u8]) -> LegacyDirectorySnapshot {
    LegacyDirectorySnapshot::LegacyTerminalInactive {
        directory: directory(),
        control_files: vec![
            file("state.json", bytes),
            file("job.json", b"historical manifest"),
            file("worker.py", b"old source only"),
        ],
        process: process(),
    }
}
fn staged() -> LegacyDirectorySnapshot {
    LegacyDirectorySnapshot::LegacyStagedNoDispatch {
        directory: directory(),
        files: [
            "job.json",
            "journal.py",
            "worker.py",
            "run-job.ps1",
            "params.json",
            "stage.json",
            "inputs/轴.prt",
        ]
        .iter()
        .map(|name| file(name, b"reviewed bytes"))
        .collect(),
        directories: vec![
            RelativePath::new("inputs").unwrap(),
            RelativePath::new("empty-input-subdir").unwrap(),
        ],
    }
}
fn review(snapshot: LegacyDirectorySnapshot) -> RootLegacyReview {
    RootLegacyReview {
        schema: ROOT_LEGACY_REVIEW_SCHEMA.into(),
        scope: LegacyReviewScope::WriterHoldReleaseOnly,
        root: root(),
        entries: vec![RootLegacyReviewEntry {
            job_id: JobId::new("old-job").unwrap(),
            snapshot,
            evidence_records: vec![RelativePath::new("inspection/raw.json").unwrap()],
            reason: "independently reviewed source and execution history".into(),
        }],
        references: vec![file(
            "inspection/raw.json",
            b"external immutable review record",
        )],
        reviewer: "authorized root review".into(),
        reviewed_at_utc: "2026-09-28T16:00:00Z".into(),
        reason: "historical writer hold only".into(),
    }
}
fn observed(kind: LegacyCurrentProcess) -> LegacyProcessObservation {
    LegacyProcessObservation {
        pid: 42,
        creation_time_100ns: process().creation_time_100ns,
        observation: kind,
    }
}
fn classify(
    r: &RootLegacyReview,
    input: &HoldInput,
    current: &LegacyDirectorySnapshot,
    process: Option<&LegacyProcessObservation>,
) -> anyhow::Result<HoldAssessment> {
    let bytes = serde_json::to_vec(r)?;
    classify_reviewed_root_legacy(
        input,
        &JobId::new("old-job")?,
        &root(),
        &bytes,
        &Digest::of(&bytes),
        current,
        process,
    )
}

#[test]
fn historical_failed_without_native_result_releases_only_writer_hold_on_current_inactivity() {
    let bytes = state(LEGACY_STATE_SCHEMA, "FAILED");
    let snapshot = terminal(&bytes);
    let r = review(snapshot.clone());
    let input = HoldInput::StateBytes(bytes.clone());
    assert!(classify_job_hold(&input, &JobId::new("old-job").unwrap(), &root()).blocks_writes);
    for kind in [
        LegacyCurrentProcess::Absent,
        LegacyCurrentProcess::DifferentCreation {
            actual_creation_time_100ns: process().creation_time_100ns + 1,
        },
        LegacyCurrentProcess::ExitedSameIdentity { exit_code: -1 },
    ] {
        let answer = classify(&r, &input, &snapshot, Some(&observed(kind))).unwrap();
        assert!(!answer.blocks_writes);
        assert!(answer.legacy_v1);
        assert_eq!(answer.state.as_deref(), Some("FAILED"));
        assert_eq!(
            answer.observed_record.unwrap(),
            json::strict_json(&bytes).unwrap()
        );
        assert!(answer.reason.ends_with("writer_hold_release_only"));
    }
}

#[test]
fn live_uncertain_missing_or_wrong_process_observations_do_not_release() {
    let bytes = state(LEGACY_STATE_SCHEMA, "COMPLETED");
    let snapshot = terminal(&bytes);
    let r = review(snapshot.clone());
    let input = HoldInput::StateBytes(bytes);
    for kind in [
        LegacyCurrentProcess::LiveSameIdentity,
        LegacyCurrentProcess::Uncertain,
        LegacyCurrentProcess::DifferentCreation {
            actual_creation_time_100ns: process().creation_time_100ns,
        },
        LegacyCurrentProcess::DifferentCreation {
            actual_creation_time_100ns: 0,
        },
    ] {
        assert!(classify(&r, &input, &snapshot, Some(&observed(kind))).is_err());
    }
    assert!(classify(&r, &input, &snapshot, None).is_err());
    let mut other = observed(LegacyCurrentProcess::Absent);
    other.pid += 1;
    assert!(classify(&r, &input, &snapshot, Some(&other)).is_err());
    other.pid = 42;
    other.creation_time_100ns += 1;
    assert!(classify(&r, &input, &snapshot, Some(&other)).is_err());
}

#[test]
fn only_missing_state_empty_or_complete_staged_snapshots_can_be_reviewed_as_undispatched() {
    for snapshot in [
        LegacyDirectorySnapshot::EmptyFailedStage {
            directory: directory(),
        },
        staged(),
    ] {
        let r = review(snapshot.clone());
        let answer = classify(&r, &HoldInput::Missing, &snapshot, None).unwrap();
        assert!(!answer.blocks_writes);
        assert_eq!(answer.observed_record, None);
        assert_eq!(answer.state, None);
        assert!(classify(&r, &HoldInput::PendingWrite, &snapshot, None).is_err());
        assert!(
            classify(
                &r,
                &HoldInput::IoUncertain("orphan tmp or reparse".into()),
                &snapshot,
                None
            )
            .is_err()
        );
        assert!(
            classify(
                &r,
                &HoldInput::StateBytes(state(LEGACY_STATE_SCHEMA, "RUNNING")),
                &snapshot,
                None
            )
            .is_err()
        );
        assert!(
            classify(
                &r,
                &HoldInput::Missing,
                &snapshot,
                Some(&observed(LegacyCurrentProcess::Absent))
            )
            .is_err()
        );
    }
}

#[test]
fn changed_directories_files_state_and_new_control_records_do_not_receive_old_exemption() {
    let bytes = state(LEGACY_STATE_SCHEMA, "FAILED");
    let snapshot = terminal(&bytes);
    let r = review(snapshot.clone());
    let input = HoldInput::StateBytes(bytes);
    let p = observed(LegacyCurrentProcess::Absent);
    let mut changed = snapshot.clone();
    if let LegacyDirectorySnapshot::LegacyTerminalInactive { directory, .. } = &mut changed {
        directory.directory_id = "recreated-job".into();
    }
    assert!(classify(&r, &input, &changed, Some(&p)).is_err());
    let mut changed = snapshot.clone();
    if let LegacyDirectorySnapshot::LegacyTerminalInactive { control_files, .. } = &mut changed {
        control_files.push(file("new-control.json", b"new"));
    }
    assert!(classify(&r, &input, &changed, Some(&p)).is_err());
    assert!(
        classify(
            &r,
            &HoldInput::StateBytes(state(LEGACY_STATE_SCHEMA, "COMPLETED")),
            &snapshot,
            Some(&p)
        )
        .is_err()
    );
    let old = staged();
    let r = review(old.clone());
    let mut changed = old.clone();
    if let LegacyDirectorySnapshot::LegacyStagedNoDispatch { files, .. } = &mut changed {
        files.push(file("inputs/new.prt", b"new"));
    }
    assert!(classify(&r, &HoldInput::Missing, &changed, None).is_err());
    let mut changed = old.clone();
    if let LegacyDirectorySnapshot::LegacyStagedNoDispatch { directories, .. } = &mut changed {
        directories.push(RelativePath::new("new-empty-directory").unwrap());
    }
    assert!(classify(&r, &HoldInput::Missing, &changed, None).is_err());
}

#[test]
fn v2_unknown_running_pending_and_cross_job_records_never_release() {
    let p = observed(LegacyCurrentProcess::Absent);
    for (schema, status) in [
        (STATE_SCHEMA, "UNKNOWN"),
        (LEGACY_STATE_SCHEMA, "RUNNING"),
        (LEGACY_STATE_SCHEMA, "UNKNOWN"),
        (LEGACY_STATE_SCHEMA, "STARTING"),
    ] {
        let bytes = state(schema, status);
        let snapshot = terminal(&bytes);
        assert!(
            classify(
                &review(snapshot.clone()),
                &HoldInput::StateBytes(bytes),
                &snapshot,
                Some(&p)
            )
            .is_err()
        );
    }
    let mut value = json::strict_json(&state(LEGACY_STATE_SCHEMA, "FAILED")).unwrap();
    value["job_id"] = json!("another-job");
    let bytes = serde_json::to_vec(&value).unwrap();
    let snapshot = terminal(&bytes);
    assert!(
        classify(
            &review(snapshot.clone()),
            &HoldInput::StateBytes(bytes),
            &snapshot,
            Some(&p)
        )
        .is_err()
    );
    let bytes = state(LEGACY_STATE_SCHEMA, "FAILED");
    let snapshot = terminal(&bytes);
    let r = review(snapshot.clone());
    for input in [
        HoldInput::PendingWrite,
        HoldInput::IoUncertain("read sharing error".into()),
    ] {
        assert!(classify(&r, &input, &snapshot, Some(&p)).is_err());
    }
}

#[test]
fn review_pin_root_unlisted_job_and_evidence_reference_are_required() {
    let snapshot = staged();
    let mut r = review(snapshot.clone());
    let bytes = serde_json::to_vec(&r).unwrap();
    assert!(
        classify_reviewed_root_legacy(
            &HoldInput::Missing,
            &JobId::new("old-job").unwrap(),
            &root(),
            &bytes,
            &Digest::of(b"wrong"),
            &snapshot,
            None
        )
        .is_err()
    );
    assert!(
        classify_reviewed_root_legacy(
            &HoldInput::Missing,
            &JobId::new("new-job").unwrap(),
            &root(),
            &bytes,
            &Digest::of(&bytes),
            &snapshot,
            None
        )
        .is_err()
    );
    let mut other = root();
    other.directory_id = "other-root".into();
    assert!(
        classify_reviewed_root_legacy(
            &HoldInput::Missing,
            &JobId::new("old-job").unwrap(),
            &other,
            &bytes,
            &Digest::of(&bytes),
            &snapshot,
            None
        )
        .is_err()
    );
    r.references.clear();
    assert!(classify(&r, &HoldInput::Missing, &snapshot, None).is_err());
    let mut value = serde_json::to_value(review(snapshot)).unwrap();
    value["scope"] = json!("engineering_accepted");
    assert!(decode::<RootLegacyReview>(&serde_json::to_vec(&value).unwrap()).is_err());
}

#[test]
fn control_selection_and_full_staged_inventory_have_distinct_boundaries() {
    let mut without_stage = staged();
    if let LegacyDirectorySnapshot::LegacyStagedNoDispatch { files, .. } = &mut without_stage {
        files.retain(|file| file.path.as_str() != "stage.json");
    }
    assert!(without_stage.validate(&root()).is_err());
    for name in [
        "STATE.JSON",
        "stage.json",
        "run-job.PS1",
        "worker.py",
        "journal.py",
        "uf_probe.rs",
        "unexpected.json",
    ] {
        assert!(legacy_terminal_control_name(name));
    }
    for name in [
        "out/result.json",
        "inputs/a.prt",
        "shape.prt",
        "bundle.zip",
        "stdout.txt",
        "image.png",
    ] {
        assert!(!legacy_terminal_control_name(name));
    }
    let mut snapshot = staged();
    if let LegacyDirectorySnapshot::LegacyStagedNoDispatch { directories, .. } = &mut snapshot {
        directories.clear();
    }
    assert!(snapshot.validate(&root()).is_err());
    let mut snapshot = staged();
    if let LegacyDirectorySnapshot::LegacyStagedNoDispatch { files, .. } = &mut snapshot {
        files.push(file("dispatch.json", b"intent"));
    }
    assert!(snapshot.validate(&root()).is_err());
    let mut snapshot = staged();
    if let LegacyDirectorySnapshot::LegacyStagedNoDispatch { files, .. } = &mut snapshot {
        files.push(file("JOB.JSON", b"alias"));
    }
    assert!(snapshot.validate(&root()).is_err());
    let snapshot = staged();
    let mut reordered = snapshot.clone();
    if let LegacyDirectorySnapshot::LegacyStagedNoDispatch {
        files, directories, ..
    } = &mut reordered
    {
        files.reverse();
        directories.reverse();
    }
    assert_eq!(snapshot.digest().unwrap(), reordered.digest().unwrap());
}

#[test]
fn timestamp_conversion_is_exact_and_rejects_invalid_calendar_and_mismatched_native_identity() {
    assert_eq!(legacy_utc_creation_time("1601-01-01T00:00:00Z").unwrap(), 0);
    assert_eq!(
        legacy_utc_creation_time("1970-01-01T00:00:00Z").unwrap(),
        116_444_736_000_000_000
    );
    assert_eq!(
        legacy_utc_creation_time(&process().native_start_utc).unwrap(),
        process().creation_time_100ns
    );
    assert_eq!(
        legacy_utc_creation_time("2023-11-14T22:13:20.1234567Z").unwrap(),
        process().creation_time_100ns + 1_234_567
    );
    assert_eq!(
        legacy_utc_creation_time("2023-11-14T22:13:20.1Z").unwrap(),
        process().creation_time_100ns + 1_000_000
    );
    assert!(legacy_utc_creation_time("2000-02-29T00:00:00Z").is_ok());
    for date in [
        "1900-02-29T00:00:00Z",
        "2023-02-29T00:00:00Z",
        "2024-04-31T00:00:00Z",
        "2024-01-01T24:00:00Z",
        "2024-01-01T00:00:60Z",
        "2024-01-01T00:00:00.12345678Z",
        "2024-01-01T00:00:00.Z",
        "2024-01-01T00:00:00+00:00",
        "2024-00-01T00:00:00Z",
    ] {
        assert!(legacy_utc_creation_time(date).is_err(), "{date}");
    }
    let mut snapshot = terminal(&state(LEGACY_STATE_SCHEMA, "FAILED"));
    if let LegacyDirectorySnapshot::LegacyTerminalInactive { process, .. } = &mut snapshot {
        process.creation_time_100ns += 1;
    }
    assert!(snapshot.validate(&root()).is_err());
}

#[test]
fn duplicate_review_json_keys_and_duplicate_directory_bindings_are_rejected() {
    let r = review(staged());
    let bytes = serde_json::to_vec(&r).unwrap();
    let mut text = String::from_utf8(bytes).unwrap();
    text = text.replacen("{", "{\"schema\":\"wrong\",", 1);
    assert!(decode::<RootLegacyReview>(text.as_bytes()).is_err());
    let mut r = r;
    r.entries.push(r.entries[0].clone());
    r.entries[1].job_id = JobId::new("other-job").unwrap();
    assert!(r.validate().is_err());
}
