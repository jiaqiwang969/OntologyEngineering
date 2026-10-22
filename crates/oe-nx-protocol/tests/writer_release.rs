//! Synthetic contract fixtures only; no real host, prior job or release authority.
use oe_nx_protocol::*;
#[path = "support/writer_release_fixture.rs"]
mod writer_release_fixture;

fn fixture() -> (
    DispatchWriterReleaseReview,
    DispatchWriterFacts,
    HoldInput,
    JobId,
) {
    fixture_from_records(writer_release_fixture::records_and_inventory())
}
fn parameter_fixture() -> (
    DispatchWriterReleaseReview,
    DispatchWriterFacts,
    HoldInput,
    JobId,
) {
    fixture_from_records(writer_release_fixture::parameter_guard_records())
}
fn fixture_from_records(
    (records, inventory): (std::collections::BTreeMap<String, Vec<u8>>, Vec<FileEntry>),
) -> (
    DispatchWriterReleaseReview,
    DispatchWriterFacts,
    HoldInput,
    JobId,
) {
    let job: JobManifest = decode(&records["job.json"]).unwrap();
    let state: StateRecord = decode(&records["state.json"]).unwrap();
    let supervisor: serde_json::Value =
        decode(&records["receipts/supervisor-process.json"]).unwrap();
    let worker = state.process.clone().unwrap();
    let supervisor: ProcessIdentity =
        serde_json::from_value(supervisor["process"].clone()).unwrap();
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
            .map(|(i, name)| ReviewedDirectoryIdentity {
                path: RelativePath::new(name).unwrap(),
                identity: directory(i as u8 + 1),
            })
            .collect(),
    };
    let review = DispatchWriterReleaseReview {
        schema: DISPATCH_WRITER_REVIEW_SCHEMA.into(),
        scope: DispatchWriterReviewScope::WriterHoldReleaseOnly,
        root: job.root.clone(),
        job_id: job.job_id.clone(),
        dispatch_id: state.dispatch.as_ref().unwrap().dispatch_id.clone(),
        manifest_digest: job.digest().unwrap(),
        worker_manifest_digest: job.worker.digest().unwrap(),
        worker: worker.clone(),
        supervisor: supervisor.clone(),
        snapshot: snapshot.clone(),
        failure: ReviewedPreNativeFailure::ModulePathStringGuardBeforeExplicitUfOrPartApi,
        source_failure_review: FileEntry {
            path: RelativePath::new("fixture/source-review.json").unwrap(),
            identity: BlobIdentity::of(b"synthetic source review; no real authorization"),
        },
        dll_diagnosis_review: FileEntry {
            path: RelativePath::new("fixture/dll-review.json").unwrap(),
            identity: BlobIdentity::of(b"synthetic diagnosis; no real authorization"),
        },
        reviewer: "pure contract fixture".into(),
        reviewed_at_utc: "2026-09-28T00:00:00Z".into(),
        reason: "Synthetic directory/process observations; never apply to actual root".into(),
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
        JobId::new("independent-new-fixture-job").unwrap(),
    )
}
fn verified(r: &DispatchWriterReleaseReview) -> VerifiedDispatchWriterReview {
    let bytes = serde_json::to_vec(r).unwrap();
    VerifiedDispatchWriterReview::new(&bytes, &Digest::of(&bytes), &r.root).unwrap()
}

#[test]
fn initial_receipt_failure_requires_versioned_exact_review_and_preserves_hold() {
    let (mut r, mut f, input, target) = fixture();
    r.schema = DISPATCH_WRITER_REVIEW_SCHEMA_V2.into();
    r.failure = ReviewedPreNativeFailure::InitialControllerReceiptGuardBeforeNativeLoad;
    f.records
        .remove("receipts/0001-source_and_dispatch_verified.json");
    f.snapshot
        .files
        .retain(|entry| entry.path.as_str() != "receipts/0001-source_and_dispatch_verified.json");
    for (name, raw) in [
        ("receipts/worker.stderr", b"native_worker_failed:unknown_initial_controller_receipt\n".to_vec()),
        ("receipts/worker-launch-environment.json",serde_json::to_vec(&serde_json::json!({"schema":"oev2.nx-worker-launch-environment/1","nx_bin":"C:/NX/NXBIN","child_path_sha256":"a".repeat(64),"scope":"worker child PATH only; parent unchanged; NXBIN variable inherited","root_cause":"launch-environment hypothesis; sole cause not established"})).unwrap()),
    ] {
        f.records.insert(name.into(), raw.clone());
        f.snapshot.files.retain(|entry| entry.path.as_str() != name);
        f.snapshot.files.push(FileEntry::from_bytes(name, &raw).unwrap());
    }
    r.snapshot = f.snapshot.clone();
    let original = f.records["state.json"].clone();
    let release = verified(&r)
        .assess(WriterAdmissionOperation::Stage, &target, &input, &f)
        .unwrap();
    assert_eq!(release.original_state, BlobIdentity::of(&original));
    assert!(classify_job_hold(&input, &r.job_id, &r.root).blocks_writes);
    let mut old = r.clone();
    old.schema = DISPATCH_WRITER_REVIEW_SCHEMA.into();
    let raw = serde_json::to_vec(&old).unwrap();
    assert!(
        VerifiedDispatchWriterReview::new(&raw, &Digest::of(&raw), &old.root).is_err(),
        "old protocol never gains a new release scope"
    );
    let mut live = f.clone();
    live.worker.observation = DispatchProcessState::LiveSameIdentity;
    rejected(&r, &live, &input, &target);
    let mut excess = f.clone();
    let name = "receipts/0001-source_and_dispatch_verified.json";
    excess.records.insert(name.into(), b"{}".to_vec());
    excess
        .snapshot
        .files
        .push(FileEntry::from_bytes(name, b"{}").unwrap());
    let mut broader = r.clone();
    broader.snapshot = excess.snapshot.clone();
    rejected(&broader, &excess, &input, &target);
    let mut changed = f.clone();
    changed.records.insert(
        "receipts/worker.stderr".into(),
        b"native_worker_failed:later_unknown\n".to_vec(),
    );
    rejected(&r, &changed, &input, &target);
    assert_eq!(f.records["state.json"], original);
}
fn rejected(
    r: &DispatchWriterReleaseReview,
    f: &DispatchWriterFacts,
    input: &HoldInput,
    target: &JobId,
) {
    assert!(
        verified(r)
            .assess(WriterAdmissionOperation::Stage, target, input, f)
            .is_err()
    );
    assert!(classify_job_hold(input, &r.job_id, &r.root).blocks_writes);
}
#[test]
fn writer_permission_is_separate_and_old_state_bytes_never_change() {
    let (r, f, input, target) = fixture();
    let original = f.records["state.json"].clone();
    let assessment = classify_job_hold(&input, &r.job_id, &r.root);
    assert!(assessment.blocks_writes);
    assert_eq!(assessment.state.as_deref(), Some("UNKNOWN_AFTER_DISPATCH"));
    let prepared = verified(&r);
    for operation in [
        WriterAdmissionOperation::Stage,
        WriterAdmissionOperation::Run,
        WriterAdmissionOperation::Supervise,
    ] {
        let release = prepared.assess(operation, &target, &input, &f).unwrap();
        assert_eq!(release.original_state, BlobIdentity::of(&original));
        assert_eq!(release.target_job_id, target);
        assert_eq!(release.operation, operation);
        assert_eq!(classify_job_hold(&input, &r.job_id, &r.root), assessment);
    }
    assert_eq!(f.records["state.json"], original);
    let job: JobManifest = decode(&f.records["job.json"]).unwrap();
    let mut state: StateRecord = decode(&original).unwrap();
    assert!(
        state
            .begin_dispatch(&job, state.dispatch.clone().unwrap())
            .is_err()
    );
    assert_eq!(
        serde_json::to_value(state).unwrap(),
        serde_json::from_slice::<serde_json::Value>(&original).unwrap()
    );
}
#[test]
fn original_target_rejected_before_any_supervise_skip() {
    let (r, _, _, _) = fixture();
    let prepared = verified(&r);
    for op in [
        WriterAdmissionOperation::Stage,
        WriterAdmissionOperation::Run,
        WriterAdmissionOperation::Supervise,
    ] {
        for name in [
            r.job_id.as_str().to_string(),
            r.job_id.as_str().to_ascii_uppercase(),
            r.job_id.as_str().replacen("phase", "PhAsE", 1),
        ] {
            assert!(
                prepared
                    .bind_admission(op, &JobId::new(name).unwrap())
                    .is_err()
            );
        }
    }
}
#[test]
fn pin_root_schema_or_duplicate_json_cannot_create_authority() {
    let (r, _, _, _) = fixture();
    let bytes = serde_json::to_vec(&r).unwrap();
    assert!(VerifiedDispatchWriterReview::new(&bytes, &Digest::of(b"wrong"), &r.root).is_err());
    let mut root = r.root.clone();
    root.directory_id = "9".repeat(32);
    assert!(VerifiedDispatchWriterReview::new(&bytes, &Digest::of(&bytes), &root).is_err());
    let mut value = serde_json::to_value(&r).unwrap();
    value["scope"] = "job_completed".into();
    let bytes = serde_json::to_vec(&value).unwrap();
    assert!(VerifiedDispatchWriterReview::new(&bytes, &Digest::of(&bytes), &r.root).is_err());
    let bytes = serde_json::to_vec(&r).unwrap();
    let mut duplicate = b"{\"schema\":\"bad\",".to_vec();
    duplicate.extend_from_slice(&bytes[1..]);
    assert!(
        VerifiedDispatchWriterReview::new(&duplicate, &Digest::of(&duplicate), &r.root).is_err()
    );
}
#[test]
fn pending_missing_or_io_never_releases_writer() {
    let (r, f, _, target) = fixture();
    for input in [
        HoldInput::PendingWrite,
        HoldInput::Missing,
        HoldInput::IoUncertain("access denied".into()),
    ] {
        rejected(&r, &f, &input, &target);
    }
}
#[test]
fn live_uncertain_wrong_or_conflicting_process_stays_held() {
    let (r, f, input, target) = fixture();
    for state in [
        DispatchProcessState::LiveSameIdentity,
        DispatchProcessState::Uncertain,
        DispatchProcessState::ExitedSameIdentity { exit_code: 0 },
        DispatchProcessState::DifferentCreation {
            actual_creation_time_100ns: r.worker.creation_time_100ns,
        },
    ] {
        let mut bad = f.clone();
        bad.worker.observation = state;
        rejected(&r, &bad, &input, &target);
    }
    let mut bad = f.clone();
    bad.worker.expected.pid += 1;
    rejected(&r, &bad, &input, &target);
    let mut bad = f.clone();
    bad.supervisor.observation = DispatchProcessState::LiveSameIdentity;
    rejected(&r, &bad, &input, &target);
    let mut reused = f.clone();
    reused.worker.observation = DispatchProcessState::DifferentCreation {
        actual_creation_time_100ns: r.worker.creation_time_100ns + 1,
    };
    assert!(
        verified(&r)
            .assess(WriterAdmissionOperation::Stage, &target, &input, &reused)
            .is_ok()
    );
}
#[test]
fn changed_directory_payload_or_empty_out_identity_stays_held() {
    let (r, f, input, target) = fixture();
    let mut bad = f.clone();
    bad.snapshot.directory.directory_id = "8".repeat(32);
    rejected(&r, &bad, &input, &target);
    let mut bad = f.clone();
    bad.snapshot.files[0].identity.sha256 = Digest::of(b"changed");
    rejected(&r, &bad, &input, &target);
    let mut bad = f.clone();
    bad.snapshot
        .directories
        .retain(|d| d.path.as_str() != "out");
    rejected(&r, &bad, &input, &target);
    let mut bad = f.clone();
    bad.snapshot.files.pop();
    rejected(&r, &bad, &input, &target);
}
fn rebind(
    r: &mut DispatchWriterReleaseReview,
    f: &mut DispatchWriterFacts,
    name: &str,
    bytes: Vec<u8>,
) {
    f.records.insert(name.into(), bytes.clone());
    f.snapshot
        .files
        .iter_mut()
        .find(|e| e.path.as_str() == name)
        .unwrap()
        .identity = BlobIdentity::of(&bytes);
    r.snapshot = f.snapshot.clone();
}
#[test]
fn even_re_pinned_forged_nonce_termination_or_event_is_rejected() {
    let (r, f, input, target) = fixture();
    for (name, key, value) in [
        (
            "receipts/worker-termination.json",
            "exit_code",
            serde_json::json!(0),
        ),
        (
            "receipts/worker-termination.json",
            "dispatch_id",
            serde_json::json!(Digest::of(b"other")),
        ),
        (
            "receipts/0001-source_and_dispatch_verified.json",
            "phase",
            serde_json::json!("initialize_intent"),
        ),
    ] {
        let mut r = r.clone();
        let mut f = f.clone();
        let mut record: serde_json::Value = decode(&f.records[name]).unwrap();
        record[key] = value;
        rebind(&mut r, &mut f, name, serde_json::to_vec(&record).unwrap());
        rejected(&r, &f, &input, &target);
    }
}
#[test]
fn result_new_event_or_output_is_not_an_eligible_pre_native_snapshot() {
    let (r, f, input, target) = fixture();
    for name in [
        "native-result.json",
        "native-result.pending.json",
        "receipts/0002-initialize_intent.json",
        "out/model.prt",
    ] {
        let mut r = r.clone();
        let mut f = f.clone();
        f.snapshot.files.push(FileEntry {
            path: RelativePath::new(name).unwrap(),
            identity: BlobIdentity::of(b"new bytes"),
        });
        r.snapshot = f.snapshot.clone();
        let raw = serde_json::to_vec(&r).unwrap();
        if let Ok(review) = VerifiedDispatchWriterReview::new(&raw, &Digest::of(&raw), &r.root) {
            assert!(
                review
                    .assess(WriterAdmissionOperation::Stage, &target, &input, &f)
                    .is_err()
            );
        }
        assert!(classify_job_hold(&input, &r.job_id, &r.root).blocks_writes);
    }
}
#[test]
fn later_state_or_different_failure_does_not_reuse_review() {
    let (r, f, input, target) = fixture();
    let mut changed = f.clone();
    changed.records.get_mut("state.json").unwrap().push(b' ');
    rejected(&r, &changed, &input, &target);
    let mut r = r.clone();
    let mut changed = f.clone();
    rebind(
        &mut r,
        &mut changed,
        "receipts/worker.stderr",
        b"native_worker_failed:part_save_failed\n".to_vec(),
    );
    rejected(&r, &changed, &input, &target);
}

#[test]
fn two_image_claims_cannot_make_one_native_instance_into_worker_and_supervisor() {
    let (mut review, _, _, _) = fixture();
    review.supervisor.pid = review.worker.pid;
    review.supervisor.creation_time_100ns = review.worker.creation_time_100ns;
    assert_ne!(review.supervisor.image, review.worker.image);
    let bytes = serde_json::to_vec(&review).unwrap();
    assert!(VerifiedDispatchWriterReview::new(&bytes, &Digest::of(&bytes), &review.root).is_err());
}

#[test]
fn exact_parameter_budget_preload_failure_uses_new_review_without_changing_old_scope() {
    let (mut review, facts, input, target) = parameter_fixture();
    review.schema = DISPATCH_WRITER_REVIEW_SCHEMA_V3.into();
    review.failure = ReviewedPreNativeFailure::LegacyStepBodiesBudgetGuardBeforeNativeLoad;
    let original = facts.records["state.json"].clone();
    assert!(
        verified(&review)
            .assess(WriterAdmissionOperation::Stage, &target, &input, &facts)
            .is_ok()
    );
    for schema in [
        DISPATCH_WRITER_REVIEW_SCHEMA,
        DISPATCH_WRITER_REVIEW_SCHEMA_V2,
    ] {
        let mut old = review.clone();
        old.schema = schema.into();
        let raw = serde_json::to_vec(&old).unwrap();
        assert!(VerifiedDispatchWriterReview::new(&raw, &Digest::of(&raw), &old.root).is_err());
    }
    let mut live = facts.clone();
    live.worker.observation = DispatchProcessState::LiveSameIdentity;
    rejected(&review, &live, &input, &target);
    for budget in [1, 128] {
        let mut changed = facts.clone();
        let mut value: serde_json::Value = decode(&changed.records["params.json"]).unwrap();
        value["maximum_bodies"] = serde_json::json!(budget);
        changed
            .records
            .insert("params.json".into(), serde_json::to_vec(&value).unwrap());
        rejected(&review, &changed, &input, &target);
    }
    let mut excess = facts.clone();
    excess.records.insert(
        "receipts/0001-source_and_dispatch_verified.json".into(),
        b"{}".to_vec(),
    );
    excess.snapshot.files.push(
        FileEntry::from_bytes("receipts/0001-source_and_dispatch_verified.json", b"{}").unwrap(),
    );
    let mut broader = review.clone();
    broader.snapshot = excess.snapshot.clone();
    rejected(&broader, &excess, &input, &target);
    assert!(
        verified(&review)
            .bind_admission(WriterAdmissionOperation::Run, &review.job_id)
            .is_err()
    );
    assert_eq!(facts.records["state.json"], original);
    assert!(classify_job_hold(&input, &review.job_id, &review.root).blocks_writes);
}

#[test]
fn coherent_parameter_scope_counterexamples_reach_the_exact_budget_predicate() {
    let original = serde_json::json!({"schema":"cad-agent.nx-native-acceptance-params/v2","action":"import_step_bodies","step":"fixture.STEP","expected_units":"mm","maximum_bodies":100000});
    let cases = [
        ("maximum_bodies", serde_json::json!(1)),
        ("maximum_bodies", serde_json::json!(128)),
        ("action", serde_json::json!("inspect")),
        ("schema", serde_json::json!("synthetic.other/v1")),
        ("expected_units", serde_json::json!("inch")),
    ];
    for (field, value) in cases {
        let mut parameters = original.clone();
        parameters[field] = value;
        let (mut review, facts, input, target) = fixture_from_records(
            writer_release_fixture::coherent_parameter_records(&parameters),
        );
        review.schema = DISPATCH_WRITER_REVIEW_SCHEMA_V3.into();
        review.failure = ReviewedPreNativeFailure::LegacyStepBodiesBudgetGuardBeforeNativeLoad;
        let raw_state = facts.records["state.json"].clone();
        let job: JobManifest = decode(&facts.records["job.json"]).unwrap();
        job.validate().unwrap();
        job.params
            .identity
            .verify(&facts.records["params.json"])
            .unwrap();
        assert_eq!(review.manifest_digest, job.digest().unwrap());
        assert_eq!(review.worker_manifest_digest, job.worker.digest().unwrap());
        assert_eq!(
            review.snapshot.digest().unwrap(),
            facts.snapshot.digest().unwrap()
        );
        let result =
            verified(&review).assess(WriterAdmissionOperation::Stage, &target, &input, &facts);
        let reason = format!("{:#}", result.unwrap_err());
        assert!(
            reason.contains("parameter guard is not the exact original legacy budget failure"),
            "{field} failed before semantic scope: {reason}"
        );
        assert_eq!(facts.records["state.json"], raw_state);
        assert!(classify_job_hold(&input, &review.job_id, &review.root).blocks_writes);
    }
}

#[test]
fn exact_parameter_guard_accepts_only_after_exit_bound_launch_receipt() {
    let (mut review, facts, input, target) = parameter_fixture();
    review.schema = DISPATCH_WRITER_REVIEW_SCHEMA_V3.into();
    review.failure = ReviewedPreNativeFailure::LegacyStepBodiesBudgetGuardBeforeNativeLoad;
    let original_state = facts.records["state.json"].clone();
    assert!(
        verified(&review)
            .assess(WriterAdmissionOperation::Stage, &target, &input, &facts)
            .is_ok()
    );
    let cases = [
        (
            "scope",
            serde_json::json!("worker child PATH only; parent unchanged; NXBIN variable inherited"),
            "launch diagnostic is not the explicitly reviewed initial-receipt variant",
        ),
        (
            "job_id",
            serde_json::json!("different-reviewed-job"),
            "after-exit launch diagnostic job/dispatch differs",
        ),
        (
            "dispatch_id",
            serde_json::json!(Digest::of(b"different-dispatch")),
            "after-exit launch diagnostic job/dispatch differs",
        ),
        (
            "schema",
            serde_json::json!("synthetic.other/v1"),
            "launch diagnostic is not the explicitly reviewed initial-receipt variant",
        ),
        (
            "root_cause",
            serde_json::json!("native qualification claimed"),
            "launch diagnostic is not the explicitly reviewed initial-receipt variant",
        ),
        (
            "child_path_sha256",
            serde_json::json!("a".repeat(63)),
            "launch diagnostic is not the explicitly reviewed initial-receipt variant",
        ),
        (
            "nx_bin",
            serde_json::json!("C:/NX/not-NXBIN-directory"),
            "launch diagnostic is not the explicitly reviewed initial-receipt variant",
        ),
    ];
    for (field, value, reason) in cases {
        let mut changed_review = review.clone();
        let mut changed = facts.clone();
        let mut launch: serde_json::Value =
            decode(&changed.records["receipts/worker-launch-environment.json"]).unwrap();
        launch[field] = value;
        rebind(
            &mut changed_review,
            &mut changed,
            "receipts/worker-launch-environment.json",
            serde_json::to_vec(&launch).unwrap(),
        );
        let error = verified(&changed_review)
            .assess(WriterAdmissionOperation::Stage, &target, &input, &changed)
            .unwrap_err();
        assert!(
            format!("{error:#}").contains(reason),
            "wrong predicate for {field}: {error:#}"
        );
        assert_eq!(changed.records["state.json"], original_state);
        assert!(
            classify_job_hold(&input, &changed_review.job_id, &changed_review.root).blocks_writes
        );
    }
    // A fully rebound initial-receipt failure still rejects the new after-exit
    // diagnostic. Its legacy scope is not broadened by the new failure branch.
    let (mut old_review, mut old_facts, old_input, old_target) = fixture();
    old_review.schema = DISPATCH_WRITER_REVIEW_SCHEMA_V2.into();
    old_review.failure = ReviewedPreNativeFailure::InitialControllerReceiptGuardBeforeNativeLoad;
    old_facts
        .records
        .remove("receipts/0001-source_and_dispatch_verified.json");
    old_facts
        .snapshot
        .files
        .retain(|entry| entry.path.as_str() != "receipts/0001-source_and_dispatch_verified.json");
    let name = "receipts/worker-launch-environment.json";
    let mut launch: serde_json::Value = decode(&facts.records[name]).unwrap();
    launch["job_id"] = serde_json::json!(old_review.job_id);
    launch["dispatch_id"] = serde_json::json!(old_review.dispatch_id);
    let raw = serde_json::to_vec(&launch).unwrap();
    old_facts.records.insert(name.into(), raw.clone());
    old_facts
        .snapshot
        .files
        .push(FileEntry::from_bytes(name, &raw).unwrap());
    rebind(
        &mut old_review,
        &mut old_facts,
        "receipts/worker.stderr",
        b"native_worker_failed:unknown_initial_controller_receipt\n".to_vec(),
    );
    let error = verified(&old_review)
        .assess(
            WriterAdmissionOperation::Stage,
            &old_target,
            &old_input,
            &old_facts,
        )
        .unwrap_err();
    assert!(
        format!("{error:#}")
            .contains("launch diagnostic is not the explicitly reviewed initial-receipt variant")
    );
    for observation in [
        DispatchProcessState::LiveSameIdentity,
        DispatchProcessState::Uncertain,
    ] {
        let mut changed = facts.clone();
        changed.supervisor.observation = observation;
        rejected(&review, &changed, &input, &target);
    }
}

fn role_parameters() -> serde_json::Value {
    serde_json::json!({"schema":"cad-agent.nx-native-assembly-params/v1","action":"create_assembly_fixture",
        "intent":{"schema":"oev2.nx-native-assembly-fixture-intent/1","oracle_sha256":"a".repeat(64),
            "root_prototype_id":"ROOT","prototypes":[{"prototype_id":"ROOT","native_units":"mm",
                "box_corner_native":[],"box_lengths_native":[]}],"instances":[]},
        "budget":{"max_occurrences":100000,"max_prototypes":4096,"max_depth":64}})
}
fn role_fixture(
    worker_id: &str,
    target: &str,
    executable: &str,
    parameters: &serde_json::Value,
) -> (
    DispatchWriterReleaseReview,
    DispatchWriterFacts,
    HoldInput,
    JobId,
) {
    let (mut review, facts, input, next) =
        fixture_from_records(writer_release_fixture::coherent_worker_role_records(
            worker_id, target, executable, parameters,
        ));
    review.schema = DISPATCH_WRITER_REVIEW_SCHEMA_V4.into();
    review.failure = ReviewedPreNativeFailure::WorkerRoleIdentityGuardBeforeNativeLoad;
    (review, facts, input, next)
}
fn original_role_fixture() -> (
    DispatchWriterReleaseReview,
    DispatchWriterFacts,
    HoldInput,
    JobId,
) {
    role_fixture(
        "oe-nx-native-assembly",
        "x86_64-pc-windows-msvc",
        "worker/oe-nx-worker.exe",
        &role_parameters(),
    )
}

#[test]
fn worker_role_preload_requires_new_scope_and_coherent_exact_original_cause() {
    let (review, facts, input, target) = original_role_fixture();
    let original = facts.records["state.json"].clone();
    for operation in [
        WriterAdmissionOperation::Stage,
        WriterAdmissionOperation::Run,
        WriterAdmissionOperation::Supervise,
    ] {
        let release = verified(&review)
            .assess(operation, &target, &input, &facts)
            .unwrap();
        assert_eq!(release.original_state, BlobIdentity::of(&original));
        assert!(
            verified(&review)
                .bind_admission(operation, &review.job_id)
                .is_err()
        );
    }
    for schema in [
        DISPATCH_WRITER_REVIEW_SCHEMA,
        DISPATCH_WRITER_REVIEW_SCHEMA_V2,
        DISPATCH_WRITER_REVIEW_SCHEMA_V3,
    ] {
        let mut old = review.clone();
        old.schema = schema.into();
        let bytes = serde_json::to_vec(&old).unwrap();
        assert!(VerifiedDispatchWriterReview::new(&bytes, &Digest::of(&bytes), &old.root).is_err());
    }
    for failure in [
        ReviewedPreNativeFailure::ModulePathStringGuardBeforeExplicitUfOrPartApi,
        ReviewedPreNativeFailure::InitialControllerReceiptGuardBeforeNativeLoad,
        ReviewedPreNativeFailure::LegacyStepBodiesBudgetGuardBeforeNativeLoad,
    ] {
        let mut old = review.clone();
        old.failure = failure;
        let bytes = serde_json::to_vec(&old).unwrap();
        assert!(VerifiedDispatchWriterReview::new(&bytes, &Digest::of(&bytes), &old.root).is_err());
    }
    let mut action = role_parameters();
    action["action"] = serde_json::json!("inspect_assembly");
    let mut schema = role_parameters();
    schema["schema"] = serde_json::json!("synthetic.other/v1");
    let cases = [
        (
            "oe-nx-native-acceptance",
            "x86_64-pc-windows-msvc",
            "worker/oe-nx-worker.exe",
            role_parameters(),
        ),
        (
            "different-invalid-role",
            "x86_64-pc-windows-msvc",
            "worker/oe-nx-worker.exe",
            role_parameters(),
        ),
        (
            "oe-nx-native-assembly",
            "x86_64-pc-windows-gnu",
            "worker/oe-nx-worker.exe",
            role_parameters(),
        ),
        (
            "oe-nx-native-assembly",
            "x86_64-pc-windows-msvc",
            "worker/different.exe",
            role_parameters(),
        ),
        (
            "oe-nx-native-assembly",
            "x86_64-pc-windows-msvc",
            "worker/oe-nx-worker.exe",
            action,
        ),
        (
            "oe-nx-native-assembly",
            "x86_64-pc-windows-msvc",
            "worker/oe-nx-worker.exe",
            schema,
        ),
    ];
    for (id, os, exe, params) in cases {
        let (review, facts, input, target) = role_fixture(id, os, exe, &params);
        let job: JobManifest = decode(&facts.records["job.json"]).unwrap();
        job.validate().unwrap();
        job.params
            .identity
            .verify(&facts.records["params.json"])
            .unwrap();
        assert_eq!(review.manifest_digest, job.digest().unwrap());
        assert_eq!(review.worker_manifest_digest, job.worker.digest().unwrap());
        assert_eq!(
            review.snapshot.digest().unwrap(),
            facts.snapshot.digest().unwrap()
        );
        let error = verified(&review)
            .assess(WriterAdmissionOperation::Stage, &target, &input, &facts)
            .unwrap_err();
        assert!(
            format!("{error:#}")
                .contains("worker role guard is not the exact original assembly role failure"),
            "coherent role counterexample hit the wrong guard: {error:#}"
        );
    }
    for (field, value, reason) in [
        (
            "scope",
            serde_json::json!("worker child PATH only; parent unchanged; NXBIN variable inherited"),
            "launch diagnostic is not the explicitly reviewed initial-receipt variant",
        ),
        (
            "job_id",
            serde_json::json!("different-job"),
            "after-exit launch diagnostic job/dispatch differs",
        ),
        (
            "dispatch_id",
            serde_json::json!(Digest::of(b"wrong-dispatch")),
            "after-exit launch diagnostic job/dispatch differs",
        ),
        (
            "schema",
            serde_json::json!("synthetic.other/v1"),
            "launch diagnostic is not the explicitly reviewed initial-receipt variant",
        ),
        (
            "root_cause",
            serde_json::json!("native success claimed"),
            "launch diagnostic is not the explicitly reviewed initial-receipt variant",
        ),
        (
            "child_path_sha256",
            serde_json::json!("a".repeat(63)),
            "launch diagnostic is not the explicitly reviewed initial-receipt variant",
        ),
        (
            "nx_bin",
            serde_json::json!("C:/NX/other"),
            "launch diagnostic is not the explicitly reviewed initial-receipt variant",
        ),
    ] {
        let mut review = review.clone();
        let mut changed = facts.clone();
        let mut launch: serde_json::Value =
            decode(&changed.records["receipts/worker-launch-environment.json"]).unwrap();
        launch[field] = value;
        rebind(
            &mut review,
            &mut changed,
            "receipts/worker-launch-environment.json",
            serde_json::to_vec(&launch).unwrap(),
        );
        let error = verified(&review)
            .assess(WriterAdmissionOperation::Stage, &target, &input, &changed)
            .unwrap_err();
        assert!(
            format!("{error:#}").contains(reason),
            "rebound launch {field}: {error:#}"
        );
    }
    for observation in [
        DispatchProcessState::LiveSameIdentity,
        DispatchProcessState::Uncertain,
    ] {
        let mut changed = facts.clone();
        changed.worker.observation = observation.clone();
        rejected(&review, &changed, &input, &target);
        changed = facts.clone();
        changed.supervisor.observation = observation;
        rejected(&review, &changed, &input, &target);
    }
    for hold in [
        HoldInput::Missing,
        HoldInput::IoUncertain("synthetic unknown OS".into()),
        HoldInput::PendingWrite,
    ] {
        assert!(
            verified(&review)
                .assess(WriterAdmissionOperation::Stage, &target, &hold, &facts)
                .is_err()
        );
    }
    for name in [
        "native-result.json",
        "native-result.pending.json",
        "receipts/0001-source_and_dispatch_verified.json",
        "receipts/0002-initialize_intent.json",
        "out/model.prt",
    ] {
        let mut review = review.clone();
        let mut changed = facts.clone();
        let bytes = b"synthetic post-preload evidence";
        changed.records.insert(name.into(), bytes.to_vec());
        changed
            .snapshot
            .files
            .push(FileEntry::from_bytes(name, bytes).unwrap());
        review.snapshot = changed.snapshot.clone();
        let raw = serde_json::to_vec(&review).unwrap();
        if let Ok(verified) =
            VerifiedDispatchWriterReview::new(&raw, &Digest::of(&raw), &review.root)
        {
            assert!(
                verified
                    .assess(WriterAdmissionOperation::Stage, &target, &input, &changed)
                    .is_err()
            );
        }
    }
    let mut wrong_exit = facts.clone();
    let mut changed_review = review.clone();
    let mut term: serde_json::Value =
        decode(&wrong_exit.records["receipts/worker-termination.json"]).unwrap();
    term["exit_code"] = serde_json::json!(0);
    rebind(
        &mut changed_review,
        &mut wrong_exit,
        "receipts/worker-termination.json",
        serde_json::to_vec(&term).unwrap(),
    );
    rejected(&changed_review, &wrong_exit, &input, &target);
    assert_eq!(facts.records["state.json"], original);
    assert!(classify_job_hold(&input, &review.job_id, &review.root).blocks_writes);
}

#[test]
fn review_set_pins_every_member_and_rejects_duplicate_or_original_targets() {
    let (mut first, first_facts, first_input, target) = parameter_fixture();
    first.schema = DISPATCH_WRITER_REVIEW_SCHEMA_V3.into();
    first.failure = ReviewedPreNativeFailure::LegacyStepBodiesBudgetGuardBeforeNativeLoad;
    let (mut second, mut second_facts, second_input, _) = original_role_fixture();
    second_facts.snapshot.directory.directory_id = format!("{:032x}", 200);
    second.snapshot = second_facts.snapshot.clone();
    let bytes1 = serde_json::to_vec(&first).unwrap();
    let bytes2 = serde_json::to_vec(&second).unwrap();
    let set = DispatchWriterReleaseSet {
        schema: DISPATCH_WRITER_REVIEW_SET_SCHEMA.into(),
        root: first.root.clone(),
        reviews: vec![
            FileEntry::from_bytes("review01.json", &bytes1).unwrap(),
            FileEntry::from_bytes("review02.json", &bytes2).unwrap(),
        ],
    };
    let bytes = serde_json::to_vec(&set).unwrap();
    let verified_set =
        VerifiedDispatchWriterReleaseSet::new(&bytes, &Digest::of(&bytes), &set.root).unwrap();
    let one = verified(&first);
    let two = verified(&second);
    assert_eq!(verified_set.reviews().len(), 2);
    for (entry, source) in verified_set.reviews().iter().zip([&bytes1, &bytes2]) {
        entry.identity.verify(source).unwrap();
    }
    assert!(verified_set.reviews()[0].identity.verify(&bytes2).is_err());
    assert!(
        VerifiedDispatchWriterReleaseSet::new(&bytes, &Digest::of(b"wrong-pin"), &set.root)
            .is_err()
    );
    for operation in [
        WriterAdmissionOperation::Stage,
        WriterAdmissionOperation::Run,
        WriterAdmissionOperation::Supervise,
    ] {
        bind_dispatch_writer_review_set(operation, &target, &[&one, &two]).unwrap();
        one.assess(operation, &target, &first_input, &first_facts)
            .unwrap();
        two.assess(operation, &target, &second_input, &second_facts)
            .unwrap();
        for id in [first.job_id.as_str(), second.job_id.as_str()] {
            assert!(
                bind_dispatch_writer_review_set(
                    operation,
                    &JobId::new(id.to_ascii_uppercase()).unwrap(),
                    &[&one, &two]
                )
                .is_err()
            );
        }
        assert!(bind_dispatch_writer_review_set(operation, &target, &[&one, &one]).is_err());
        assert!(bind_dispatch_writer_review_set(operation, &target, &[]).is_err());
    }
    for (field, value) in [
        ("schema", serde_json::json!("synthetic.set/v0")),
        ("reviews", serde_json::json!([])),
    ] {
        let mut changed = serde_json::to_value(&set).unwrap();
        changed[field] = value;
        let raw = serde_json::to_vec(&changed).unwrap();
        assert!(VerifiedDispatchWriterReleaseSet::new(&raw, &Digest::of(&raw), &set.root).is_err());
    }
    let mut duplicate = set.clone();
    duplicate.reviews.push(duplicate.reviews[0].clone());
    assert!(duplicate.validate(&set.root).is_err());
    let mut excessive = set.clone();
    excessive.reviews = (0..33)
        .map(|i| FileEntry::from_bytes(&format!("review{i:02}.json"), &bytes1).unwrap())
        .collect();
    assert!(
        format!("{:#}", excessive.validate(&set.root).unwrap_err())
            .contains("dispatch review set cardinality")
    );
    let mut other = set.root.clone();
    other.directory_id = "other-root".into();
    assert!(set.validate(&other).is_err());
    let mut same_directory = second.clone();
    same_directory.snapshot.directory = first.snapshot.directory.clone();
    let same = verified(&same_directory);
    assert!(
        bind_dispatch_writer_review_set(
            WriterAdmissionOperation::Supervise,
            &target,
            &[&one, &same]
        )
        .is_err()
    );
    let mut mixed_root = second.clone();
    mixed_root.root.directory_id = "other-root".into();
    let other = verified(&mixed_root);
    assert!(
        bind_dispatch_writer_review_set(WriterAdmissionOperation::Stage, &target, &[&one, &other])
            .is_err()
    );
    assert!(classify_job_hold(&first_input, &first.job_id, &first.root).blocks_writes);
    assert!(classify_job_hold(&second_input, &second.job_id, &second.root).blocks_writes);
}
