//! Independent v5 admission controls. All identities and records are synthetic.
//! This file creates no actual release, OS evidence, native result or state change.
use oe_nx_protocol::*;
use serde_json::{Value, json};

#[path = "support/writer_release_fixture.rs"]
mod writer_release_fixture;

fn params(action: &str, schema: &str) -> Value {
    json!({"schema":schema,"action":action,
        "intent":{"path":"independent/CYLINDER-A-PRODUCT-INTENT-01.json","sha256":"a".repeat(64)}})
}

fn fixture_with(
    role: &str,
    target: &str,
    executable: &str,
    action: &str,
    schema: &str,
) -> (
    DispatchWriterReleaseReview,
    DispatchWriterFacts,
    HoldInput,
    JobId,
) {
    let (records, inventory) = writer_release_fixture::coherent_worker_role_records(
        role,
        target,
        executable,
        &params(action, schema),
    );
    let job: JobManifest = decode(&records["job.json"]).unwrap();
    let state: StateRecord = decode(&records["state.json"]).unwrap();
    let supervisor: Value = decode(&records["receipts/supervisor-process.json"]).unwrap();
    let supervisor: ProcessIdentity =
        serde_json::from_value(supervisor["process"].clone()).unwrap();
    let worker = state.process.clone().unwrap();
    let directory = |n: u8| RootIdentity {
        host_id: job.root.host_id.clone(),
        volume_id: job.root.volume_id.clone(),
        directory_id: format!("{n:032x}"),
    };
    let snapshot = DispatchWriterSnapshot {
        directory: directory(100),
        files: inventory,
        directories: ["build", "out", "receipts", "sources", "worker"]
            .into_iter()
            .enumerate()
            .map(|(i, path)| ReviewedDirectoryIdentity {
                path: RelativePath::new(path).unwrap(),
                identity: directory(i as u8 + 1),
            })
            .collect(),
    };
    let review = DispatchWriterReleaseReview {
        schema: DISPATCH_WRITER_REVIEW_SCHEMA_V5.into(),
        scope: DispatchWriterReviewScope::WriterHoldReleaseOnly,
        root: job.root.clone(),
        job_id: job.job_id.clone(),
        dispatch_id: state.dispatch.as_ref().unwrap().dispatch_id.clone(),
        manifest_digest: job.digest().unwrap(),
        worker_manifest_digest: job.worker.digest().unwrap(),
        worker: worker.clone(),
        supervisor: supervisor.clone(),
        snapshot: snapshot.clone(),
        failure: ReviewedPreNativeFailure::CylinderWorkerRoleIdentityGuardBeforeNativeLoad,
        source_failure_review: FileEntry::from_bytes(
            "fixture/source-review.json",
            b"synthetic source review only",
        )
        .unwrap(),
        dll_diagnosis_review: FileEntry::from_bytes(
            "fixture/dll-review.json",
            b"synthetic DLL review only",
        )
        .unwrap(),
        reviewer: "independent synthetic cylinder controls".into(),
        reviewed_at_utc: "2026-10-07T00:00:00Z".into(),
        reason: "Finite contract control; never authority for a real machine".into(),
    };
    let input = HoldInput::StateBytes(records["state.json"].clone());
    (
        review,
        DispatchWriterFacts {
            snapshot,
            records,
            worker: DispatchProcessObservation {
                expected: worker,
                observation: DispatchProcessState::ExitedSameIdentity { exit_code: 1 },
            },
            supervisor: DispatchProcessObservation {
                expected: supervisor,
                observation: DispatchProcessState::Absent,
            },
        },
        input,
        JobId::new("independent-cylinder-new-job").unwrap(),
    )
}

fn fixture() -> (
    DispatchWriterReleaseReview,
    DispatchWriterFacts,
    HoldInput,
    JobId,
) {
    fixture_with(
        "oe-nx-rotating-part",
        "x86_64-pc-windows-msvc",
        "worker/oe-nx-worker.exe",
        "create_cylinder_solid",
        "oev2.nx-rotating-part-params/1",
    )
}

fn assess(
    review: &DispatchWriterReleaseReview,
    facts: &DispatchWriterFacts,
    input: &HoldInput,
    target: &JobId,
    op: WriterAdmissionOperation,
) -> anyhow::Result<WriterAdmissionRelease> {
    let raw = serde_json::to_vec(review).unwrap();
    VerifiedDispatchWriterReview::new(&raw, &Digest::of(&raw), &review.root)?
        .assess(op, target, input, facts)
}

fn rejected(
    review: &DispatchWriterReleaseReview,
    facts: &DispatchWriterFacts,
    input: &HoldInput,
    target: &JobId,
) {
    let old = facts.records["state.json"].clone();
    assert!(
        assess(
            review,
            facts,
            input,
            target,
            WriterAdmissionOperation::Stage
        )
        .is_err()
    );
    assert!(classify_job_hold(input, &review.job_id, &review.root).blocks_writes);
    assert_eq!(facts.records["state.json"], old);
}

fn put(facts: &mut DispatchWriterFacts, path: &str, raw: Vec<u8>) {
    facts
        .snapshot
        .files
        .retain(|entry| entry.path.as_str() != path);
    facts
        .snapshot
        .files
        .push(FileEntry::from_bytes(path, &raw).unwrap());
    facts.records.insert(path.into(), raw);
}

#[test]
fn exact_v5_releases_only_a_new_writer_and_keeps_original_unknown_bytes() {
    let (review, facts, input, target) = fixture();
    let original = facts.records["state.json"].clone();
    let original_hold = classify_job_hold(&input, &review.job_id, &review.root);
    assert_eq!(
        original_hold.state.as_deref(),
        Some("UNKNOWN_AFTER_DISPATCH")
    );
    for op in [
        WriterAdmissionOperation::Stage,
        WriterAdmissionOperation::Run,
        WriterAdmissionOperation::Supervise,
    ] {
        let release = assess(&review, &facts, &input, &target, op).unwrap();
        assert_eq!(release.original_state, BlobIdentity::of(&original));
        assert_eq!(release.target_job_id, target);
        assert_eq!(release.reviewed_job_id, review.job_id);
        assert_eq!(
            classify_job_hold(&input, &review.job_id, &review.root),
            original_hold
        );
    }
    assert_eq!(facts.records["state.json"], original);
}

#[test]
fn old_versions_refuse_cylinder_cause_and_v5_cannot_reinterpret_assembly() {
    let (review, facts, input, target) = fixture();
    for schema in [
        DISPATCH_WRITER_REVIEW_SCHEMA,
        DISPATCH_WRITER_REVIEW_SCHEMA_V2,
        DISPATCH_WRITER_REVIEW_SCHEMA_V3,
        DISPATCH_WRITER_REVIEW_SCHEMA_V4,
    ] {
        let mut old = review.clone();
        old.schema = schema.into();
        rejected(&old, &facts, &input, &target);
    }
    let (mut assembly, facts, input, target) = fixture_with(
        "oe-nx-native-assembly",
        "x86_64-pc-windows-msvc",
        "worker/oe-nx-worker.exe",
        "create_assembly_fixture",
        "cad-agent.nx-native-assembly-params/v1",
    );
    assembly.failure = ReviewedPreNativeFailure::WorkerRoleIdentityGuardBeforeNativeLoad;
    rejected(&assembly, &facts, &input, &target);
    assembly.schema = DISPATCH_WRITER_REVIEW_SCHEMA_V4.into();
    assert!(
        assess(
            &assembly,
            &facts,
            &input,
            &target,
            WriterAdmissionOperation::Stage
        )
        .is_ok(),
        "original assembly v4 purpose must remain valid"
    );
}

#[test]
fn coherently_rebound_wrong_role_target_path_action_or_schema_still_refuses_v5() {
    for (role, target, executable, action, schema) in [
        (
            "oe-nx-native-assembly",
            "x86_64-pc-windows-msvc",
            "worker/oe-nx-worker.exe",
            "create_cylinder_solid",
            "oev2.nx-rotating-part-params/1",
        ),
        (
            "oe-nx-native-acceptance",
            "x86_64-pc-windows-msvc",
            "worker/oe-nx-worker.exe",
            "create_cylinder_solid",
            "oev2.nx-rotating-part-params/1",
        ),
        (
            "oe-nx-rotating-part",
            "x86_64-pc-windows-gnu",
            "worker/oe-nx-worker.exe",
            "create_cylinder_solid",
            "oev2.nx-rotating-part-params/1",
        ),
        (
            "oe-nx-rotating-part",
            "x86_64-pc-windows-msvc",
            "worker/other.exe",
            "create_cylinder_solid",
            "oev2.nx-rotating-part-params/1",
        ),
        (
            "oe-nx-rotating-part",
            "x86_64-pc-windows-msvc",
            "worker/oe-nx-worker.exe",
            "inspect_cylinder_solid",
            "oev2.nx-rotating-part-params/1",
        ),
        (
            "oe-nx-rotating-part",
            "x86_64-pc-windows-msvc",
            "worker/oe-nx-worker.exe",
            "create_assembly_fixture",
            "cad-agent.nx-native-assembly-params/v1",
        ),
        (
            "oe-nx-rotating-part",
            "x86_64-pc-windows-msvc",
            "worker/oe-nx-worker.exe",
            "create_cylinder_solid",
            "cad-agent.nx-native-acceptance-params/v2",
        ),
    ] {
        // The helper rebuilds job/state/dispatch/stage/termination/inventory hashes.
        // A rejection therefore cannot rely solely on stale records.
        let (review, facts, input, target) = fixture_with(role, target, executable, action, schema);
        rejected(&review, &facts, &input, &target);
    }
}

#[test]
fn any_output_native_event_result_or_launch_variant_is_outside_v5_scope() {
    let (review, facts, input, target) = fixture();
    for path in [
        "out/cylinder.prt",
        "native-result.json",
        "receipts/0001-source_and_dispatch_verified.json",
        "receipts/0002-native-initialize.json",
    ] {
        let mut extra = facts.clone();
        put(&mut extra, path, b"{}".to_vec());
        let mut rebound = review.clone();
        rebound.snapshot = extra.snapshot.clone();
        rejected(&rebound, &extra, &input, &target);
    }
    for field in [
        "job_id",
        "dispatch_id",
        "scope",
        "root_cause",
        "child_path_sha256",
    ] {
        let mut changed = facts.clone();
        let mut launch: Value =
            decode(&changed.records["receipts/worker-launch-environment.json"]).unwrap();
        launch[field] = json!("wrong");
        put(
            &mut changed,
            "receipts/worker-launch-environment.json",
            serde_json::to_vec(&launch).unwrap(),
        );
        let mut rebound = review.clone();
        rebound.snapshot = changed.snapshot.clone();
        rejected(&rebound, &changed, &input, &target);
    }
}

#[test]
fn os_identity_is_exact_and_real_pid_reuse_is_not_original_worker_liveness() {
    let (review, facts, input, target) = fixture();
    for observation in [
        DispatchProcessState::LiveSameIdentity,
        DispatchProcessState::Uncertain,
        DispatchProcessState::ExitedSameIdentity { exit_code: 0 },
        DispatchProcessState::DifferentCreation {
            actual_creation_time_100ns: 0,
        },
        DispatchProcessState::DifferentCreation {
            actual_creation_time_100ns: review.worker.creation_time_100ns,
        },
    ] {
        let mut bad = facts.clone();
        bad.worker.observation = observation;
        rejected(&review, &bad, &input, &target);
    }
    let mut live_supervisor = facts.clone();
    live_supervisor.supervisor.observation = DispatchProcessState::LiveSameIdentity;
    rejected(&review, &live_supervisor, &input, &target);
    for change in 0..3 {
        let mut wrong = facts.clone();
        match change {
            0 => wrong.worker.expected.pid += 1,
            1 => wrong.worker.expected.creation_time_100ns += 1,
            _ => wrong.worker.expected.image = BlobIdentity::of(b"another worker image"),
        }
        rejected(&review, &wrong, &input, &target);
    }
    // Preserve the existing exact-identity retirement meaning: the old process
    // is gone when a trusted OS observation finds a valid different creation.
    // This pure checker neither controls the new process nor retries the old job.
    let mut recycled = facts.clone();
    recycled.worker.observation = DispatchProcessState::DifferentCreation {
        actual_creation_time_100ns: review.worker.creation_time_100ns + 1,
    };
    assert!(
        assess(
            &review,
            &recycled,
            &input,
            &target,
            WriterAdmissionOperation::Stage
        )
        .is_ok()
    );
    assert_eq!(recycled.records["state.json"], facts.records["state.json"]);
}

#[test]
fn snapshot_or_control_drift_and_uncertain_state_keep_the_writer_held() {
    let (review, facts, input, target) = fixture();
    let mut directory = facts.clone();
    directory.snapshot.directory.directory_id = "9".repeat(32);
    rejected(&review, &directory, &input, &target);
    let mut changed = facts.clone();
    changed.records.insert(
        "receipts/worker.stderr".into(),
        b"different failure\n".to_vec(),
    );
    rejected(&review, &changed, &input, &target);
    let mut missing = facts.clone();
    missing.records.remove("receipts/worker-termination.json");
    rejected(&review, &missing, &input, &target);
    for uncertain in [
        HoldInput::PendingWrite,
        HoldInput::Missing,
        HoldInput::IoUncertain("access denied".into()),
    ] {
        assert!(
            assess(
                &review,
                &facts,
                &uncertain,
                &target,
                WriterAdmissionOperation::Stage
            )
            .is_err()
        );
        assert!(classify_job_hold(&uncertain, &review.job_id, &review.root).blocks_writes);
    }
}

#[test]
fn original_job_and_case_alias_never_become_new_targets_for_any_operation() {
    let (review, facts, input, _) = fixture();
    for op in [
        WriterAdmissionOperation::Stage,
        WriterAdmissionOperation::Run,
        WriterAdmissionOperation::Supervise,
    ] {
        for name in [
            review.job_id.as_str().to_owned(),
            review.job_id.as_str().to_ascii_uppercase(),
        ] {
            let old_target = JobId::new(name).unwrap();
            assert!(assess(&review, &facts, &input, &old_target, op).is_err());
        }
    }
    assert!(classify_job_hold(&input, &review.job_id, &review.root).blocks_writes);
}
