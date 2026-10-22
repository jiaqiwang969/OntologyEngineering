#![cfg(windows)]
include!("support/native_payload_fixture.rs");
use oe_windows_controller::{
    adapter,
    native_stage::{self, Checkpoint, Operation},
    transfer_reconstruct_windows::PinnedTarget,
    windows::{PinnedDirectory, STATE_NAME, StableRoot, WriterLock, observe_job_records},
};
use std::ffi::OsStr;
fn temporary_root() -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!(
        "oe-atomic-stage-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&p).unwrap();
    p
}
fn prepared_job<'r>(
    root: &'r StableRoot,
    lock: &WriterLock<'r>,
    name: &str,
) -> (
    JobManifest,
    PublishedPayloadPlan,
    PreparedPayload,
    PinnedDirectory<'r>,
) {
    let (mut job, _, mut sources) = fixture();
    job.root = adapter::root_identity(root);
    job.job_id = JobId::new(name).unwrap();
    for s in &mut sources {
        for id in [
            &mut s.record.source_root,
            &mut s.record.target,
            &mut s.record.publication_root,
        ] {
            id.host_id = job.root.host_id.clone();
            id.volume_id = job.root.volume_id.clone();
        }
    }
    let plan = PublishedPayloadPlan {
        schema: "oev2.nx-published-payload-plan/1".into(),
        job_manifest_digest: job.digest().unwrap(),
        sources: sources
            .iter()
            .enumerate()
            .map(|(i, s)| PublishedPayloadSource {
                publication: s.record.clone(),
                files: s.files.clone(),
                mount: if i == 0 {
                    Mount::Inputs
                } else {
                    Mount::Payload
                },
            })
            .collect(),
    };
    let dir = root.create_directory_new(OsStr::new(name)).unwrap();
    let receipt = {
        let mut target = PinnedTarget::new(&dir, lock).unwrap();
        let mut refs = sources
            .iter_mut()
            .map(|s| s as &mut dyn CopySource)
            .collect::<Vec<_>>();
        prepare(&job, &plan, &mut refs, &mut target).unwrap()
    };
    (job, plan, receipt, dir)
}
fn root_hold(root: &StableRoot, name: &str) -> bool {
    let observed = observe_job_records(root, OsStr::new(name));
    classify_hold(&adapter::hold_input(&observed)).blocks_writes
}
#[test]
fn windows_stage_commits_only_actual_complete_payload() {
    let path = temporary_root();
    {
        let root = StableRoot::open(&path, None).unwrap();
        let lock = root.lock().unwrap();
        let (job, plan, receipt, dir) = prepared_job(&root, &lock, "complete-job");
        assert!(
            native_stage::stage(&dir, &lock, &job, &plan, &receipt, Operation::Inspect).is_err()
        );
        let state =
            native_stage::stage(&dir, &lock, &job, &plan, &receipt, Operation::Commit).unwrap();
        assert_eq!(state.state, State::Staged);
        assert!(!root_hold(&root, job.job_id.as_str()));
        assert_eq!(
            native_stage::stage(&dir, &lock, &job, &plan, &receipt, Operation::Inspect).unwrap(),
            state
        );
        let mut writes = 0;
        assert_eq!(
            native_stage::stage_with_checkpoint(
                &dir,
                &lock,
                &job,
                &plan,
                &receipt,
                Operation::Commit,
                |_| {
                    writes += 1;
                    Ok(())
                }
            )
            .unwrap(),
            state
        );
        assert_eq!(writes, 0);
        let before = std::fs::read(dir.path().join(STATE_NAME)).unwrap();
        let params = std::fs::read(dir.path().join("params.json")).unwrap();
        std::fs::write(dir.path().join("params.json"), b"corrupt").unwrap();
        assert!(
            native_stage::stage(&dir, &lock, &job, &plan, &receipt, Operation::Inspect).is_err()
        );
        assert_eq!(std::fs::read(dir.path().join(STATE_NAME)).unwrap(), before);
        std::fs::write(dir.path().join("params.json"), params).unwrap();
        let mut wrong = receipt.clone();
        wrong.target.directory_id = "wrong-directory".into();
        assert!(native_stage::stage(&dir, &lock, &job, &plan, &wrong, Operation::Commit).is_err());
        std::fs::write(dir.path().join("extra"), b"x").unwrap();
        assert!(
            native_stage::stage(&dir, &lock, &job, &plan, &receipt, Operation::Recover).is_err()
        );
    }
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn windows_stage_recovers_all_checkpoints_without_losing_originals() {
    let path = temporary_root();
    {
        let root = StableRoot::open(&path, None).unwrap();
        let lock = root.lock().unwrap();
        for (i, stop) in [
            Checkpoint::ReceiptSynced,
            Checkpoint::PendingSynced,
            Checkpoint::TemporarySynced,
            Checkpoint::ReplacedAndReadback,
        ]
        .into_iter()
        .enumerate()
        {
            let (job, plan, receipt, dir) = prepared_job(&root, &lock, &format!("interrupted-{i}"));
            assert!(
                native_stage::stage_with_checkpoint(
                    &dir,
                    &lock,
                    &job,
                    &plan,
                    &receipt,
                    Operation::Commit,
                    |at| if at == stop {
                        Err(std::io::Error::other("injected interruption"))
                    } else {
                        Ok(())
                    }
                )
                .is_err()
            );
            assert!(root_hold(&root, job.job_id.as_str()));
            assert!(
                native_stage::stage(&dir, &lock, &job, &plan, &receipt, Operation::Inspect)
                    .is_err()
            );
            assert!(
                native_stage::stage(&dir, &lock, &job, &plan, &receipt, Operation::Commit).is_err()
            );
            let temp = std::fs::read_dir(dir.path())
                .unwrap()
                .filter_map(|x| {
                    let n = x.unwrap().file_name().into_string().unwrap();
                    (n.starts_with(".nx-state-") && n.ends_with(".tmp")).then_some(n)
                })
                .next();
            assert_eq!(temp.is_some(), stop == Checkpoint::TemporarySynced);
            let original = temp.as_ref().map(|n| {
                let mut p = dir
                    .pin_file(OsStr::new(n), MAX_RECORD_BYTES as u64)
                    .unwrap()
                    .unwrap();
                (
                    p.identity().clone(),
                    p.read_all(MAX_RECORD_BYTES as u64).unwrap(),
                )
            });
            if stop == Checkpoint::TemporarySynced {
                assert!(
                    native_stage::stage_with_checkpoint(
                        &dir,
                        &lock,
                        &job,
                        &plan,
                        &receipt,
                        Operation::Recover,
                        |at| if at == Checkpoint::HistoryRenamed {
                            Err(std::io::Error::other("interrupted after archive rename"))
                        } else {
                            Ok(())
                        }
                    )
                    .is_err()
                );
                assert!(root_hold(&root, job.job_id.as_str()));
            }
            let state = native_stage::stage(&dir, &lock, &job, &plan, &receipt, Operation::Recover)
                .unwrap();
            assert_eq!(state.state, State::Staged);
            assert!(!root_hold(&root, job.job_id.as_str()));
            assert_eq!(
                native_stage::stage(&dir, &lock, &job, &plan, &receipt, Operation::Inspect)
                    .unwrap(),
                state
            );
            if let (Some(name), Some((id, bytes))) = (temp, original) {
                let archive = format!(
                    "stage-recovery-{}.json",
                    name.strip_prefix(".nx-state-")
                        .unwrap()
                        .strip_suffix(".tmp")
                        .unwrap()
                );
                let mut pin = dir
                    .pin_file(OsStr::new(&archive), MAX_RECORD_BYTES as u64)
                    .unwrap()
                    .unwrap();
                assert_eq!(*pin.identity(), id);
                assert_eq!(pin.read_all(MAX_RECORD_BYTES as u64).unwrap(), bytes);
                assert!(!dir.path().join(name).exists());
            }
            println!("actual_stage_checkpoint_{i}=recovered_root_hold_cleared");
        }
    }
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn windows_stage_rejects_partial_records_and_archive_collisions() {
    let path = temporary_root();
    {
        let root = StableRoot::open(&path, None).unwrap();
        let lock = root.lock().unwrap();
        let (job, plan, receipt, dir) = prepared_job(&root, &lock, "partial-receipt");
        std::fs::write(dir.path().join("stage-receipt.json"), b"partial").unwrap();
        assert!(
            native_stage::stage(&dir, &lock, &job, &plan, &receipt, Operation::Recover).is_err()
        );
        assert_eq!(
            std::fs::read(dir.path().join("stage-receipt.json")).unwrap(),
            b"partial"
        );
        assert!(!dir.path().join(STATE_NAME).exists());
        let (job, plan, receipt, dir) = prepared_job(&root, &lock, "archive-collision");
        assert!(
            native_stage::stage_with_checkpoint(
                &dir,
                &lock,
                &job,
                &plan,
                &receipt,
                Operation::Commit,
                |at| if at == Checkpoint::TemporarySynced {
                    Err(std::io::Error::other("stop"))
                } else {
                    Ok(())
                }
            )
            .is_err()
        );
        let temp = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .find(|n| (n.starts_with(".nx-state-") && n.ends_with(".tmp")))
            .unwrap();
        let archive = format!(
            "stage-recovery-{}.json",
            temp.strip_prefix(".nx-state-")
                .unwrap()
                .strip_suffix(".tmp")
                .unwrap()
        );
        let bytes = std::fs::read(dir.path().join(&temp)).unwrap();
        std::fs::write(dir.path().join(&archive), &bytes).unwrap();
        assert!(
            native_stage::stage(&dir, &lock, &job, &plan, &receipt, Operation::Recover).is_err()
        );
        assert_eq!(std::fs::read(dir.path().join(&temp)).unwrap(), bytes);
        assert_eq!(std::fs::read(dir.path().join(&archive)).unwrap(), bytes);
        assert!(root_hold(&root, job.job_id.as_str()));
        let (job, plan, receipt, dir) = prepared_job(&root, &lock, "missing-payload");
        std::fs::remove_file(dir.path().join("worker/fixture.exe")).unwrap();
        assert!(
            native_stage::stage(&dir, &lock, &job, &plan, &receipt, Operation::Commit).is_err()
        );
        assert!(!dir.path().join("stage-receipt.json").exists());
    }
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn windows_published_origin_pins_records_and_rechecks_state() {
    use oe_windows_controller::published_origin::PublishedOrigin;
    let path = temporary_root();
    {
        let root = StableRoot::open(&path, None).unwrap();
        let lock = root.lock().unwrap();
        let (job, plan, receipt, dir) = prepared_job(&root, &lock, "origin-review");
        let state =
            native_stage::stage(&dir, &lock, &job, &plan, &receipt, Operation::Commit).unwrap();
        let review = PublishedOrigin::observe(&root, &dir, &job, &state).unwrap();
        review.validate_state(&job, &state).unwrap();
        assert!(std::fs::write(dir.path().join("source-plan.json"), b"tamper").is_err());
        assert!(std::fs::write(dir.path().join("stage-receipt.json"), b"tamper").is_err());
        let mut changed = state.clone();
        changed.source_admission.as_mut().unwrap().payload_digest = Digest::of(b"changed");
        assert!(review.validate_state(&job, &changed).is_err());
        let other = root
            .create_directory_new(OsStr::new("wrong-directory"))
            .unwrap();
        assert!(PublishedOrigin::observe(&root, &other, &job, &state).is_err());
        drop(review);
        let raw = std::fs::read(dir.path().join("source-plan.json")).unwrap();
        let mut other = plan.clone();
        other.sources[0].publication.plan_digest = Digest::of(b"other valid source plan");
        std::fs::write(
            dir.path().join("source-plan.json"),
            serde_json::to_vec(&other).unwrap(),
        )
        .unwrap();
        assert!(PublishedOrigin::observe(&root, &dir, &job, &state).is_err());
        std::fs::write(dir.path().join("source-plan.json"), raw).unwrap();
        let raw = std::fs::read(dir.path().join("stage-receipt.json")).unwrap();
        std::fs::write(dir.path().join("stage-receipt.json"), b"partial").unwrap();
        assert!(PublishedOrigin::observe(&root, &dir, &job, &state).is_err());
        std::fs::write(dir.path().join("stage-receipt.json"), raw).unwrap();
        PublishedOrigin::observe(&root, &dir, &job, &state)
            .unwrap()
            .verify()
            .unwrap();
    }
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn windows_job_inventory_streams_over_legacy_limit_and_enforces_each_bound() {
    use sha2::{Digest as _, Sha256};
    let path = temporary_root();
    {
        let root = StableRoot::open(&path, None).unwrap();
        let dir = root.create_directory_new(OsStr::new("inventory")).unwrap();
        let size = MAX_PAYLOAD_BYTES + 17;
        let file = std::fs::File::create(dir.path().join("large.bin")).unwrap();
        file.set_len(size).unwrap();
        file.sync_all().unwrap();
        drop(file);
        let mut hasher = Sha256::new();
        let buffer = [0u8; 65536];
        let mut left = size;
        while left > 0 {
            let n = left.min(buffer.len() as u64) as usize;
            hasher.update(&buffer[..n]);
            left -= n as u64;
        }
        let expected = Digest::new(format!("{:x}", hasher.finalize())).unwrap();
        let job = large();
        let (files, pins) = adapter::inventory_job_tree(&dir, "inputs/", &job, false).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].identity.bytes, size);
        assert_eq!(files[0].identity.sha256, expected);
        for p in pins {
            p.verify().unwrap();
        }
        assert!(adapter::inventory_tree(&dir, "inputs/", size).is_err());
        let old = super_legacy_job();
        assert!(adapter::inventory_job_tree(&dir, "inputs/", &old, false).is_err());
        let mut tight = job.clone();
        tight.capacity.as_mut().unwrap().0.max_input_file_bytes = MAX_PAYLOAD_BYTES;
        tight.validate().unwrap();
        assert!(adapter::inventory_job_tree(&dir, "inputs/", &tight, false).is_err());
        let count = job.payload_files().len();
        for i in 0..count {
            std::fs::write(dir.path().join(format!("extra-{i}.bin")), b"extra").unwrap();
        }
        let mut tight = job.clone();
        tight.capacity.as_mut().unwrap().0.max_input_files = count as u64;
        tight.validate().unwrap();
        assert!(adapter::inventory_job_tree(&dir, "inputs/", &tight, false).is_err());
        let mut tight = job.clone();
        tight.capacity.as_mut().unwrap().0.max_input_bytes = size;
        tight.capacity.as_mut().unwrap().0.max_input_file_bytes = size;
        tight.validate().unwrap();
        assert!(adapter::inventory_job_tree(&dir, "inputs/", &tight, false).is_err());
        let mut tight = job;
        tight.capacity.as_mut().unwrap().0.max_output_file_bytes = MAX_PAYLOAD_BYTES;
        tight.validate().unwrap();
        assert!(adapter::inventory_job_tree(&dir, "out/", &tight, true).is_err());
        println!(
            "actual_inventory_bytes={size};streamed_hash_verified;legacy_file_total_count_output_limits_rejected"
        );
    }
    std::fs::remove_dir_all(path).unwrap();
}
fn super_legacy_job() -> JobManifest {
    job()
}

#[test]
fn windows_recovered_history_is_pinned_original_staged_state() {
    use oe_windows_controller::published_origin::PublishedOrigin;
    let path = temporary_root();
    {
        let root = StableRoot::open(&path, None).unwrap();
        let lock = root.lock().unwrap();
        let (job, plan, receipt, dir) = prepared_job(&root, &lock, "history-review");
        assert!(
            native_stage::stage_with_checkpoint(
                &dir,
                &lock,
                &job,
                &plan,
                &receipt,
                Operation::Commit,
                |at| if at == Checkpoint::TemporarySynced {
                    Err(std::io::Error::other("test stop"))
                } else {
                    Ok(())
                }
            )
            .is_err()
        );
        let state =
            native_stage::stage(&dir, &lock, &job, &plan, &receipt, Operation::Recover).unwrap();
        let name = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .find(|n| n.starts_with("stage-recovery-"))
            .unwrap();
        let original = std::fs::read(dir.path().join(&name)).unwrap();
        assert_eq!(original, serde_json::to_vec(&state).unwrap());
        let origin = PublishedOrigin::observe(&root, &dir, &job, &state).unwrap();
        assert!(origin.admits_history(&name));
        origin.verify_directory(&dir).unwrap();
        assert!(std::fs::write(dir.path().join(&name), b"changed").is_err());
        std::fs::write(dir.path().join("stage-recovery-1-2.json"), &original).unwrap();
        assert!(origin.verify_directory(&dir).is_err());
        std::fs::remove_file(dir.path().join("stage-recovery-1-2.json")).unwrap();
        origin.verify_directory(&dir).unwrap();
        drop(origin);
        let prepared = StateRecord::prepared(&job).unwrap();
        std::fs::write(
            dir.path().join(&name),
            serde_json::to_vec(&prepared).unwrap(),
        )
        .unwrap();
        assert!(PublishedOrigin::observe(&root, &dir, &job, &state).is_err());
        std::fs::write(dir.path().join(&name), &original).unwrap();
        std::fs::write(
            dir.path().join("stage-recovery-not-a-record.json"),
            &original,
        )
        .unwrap();
        assert!(PublishedOrigin::observe(&root, &dir, &job, &state).is_err());
        std::fs::remove_file(dir.path().join("stage-recovery-not-a-record.json")).unwrap();
        let origin = PublishedOrigin::observe(&root, &dir, &job, &state).unwrap();
        origin.validate_state(&job, &state).unwrap();
        origin.verify_directory(&dir).unwrap();
        assert!(!dir.path().join("dispatch.json").exists());
    }
    std::fs::remove_dir_all(path).unwrap();
}
