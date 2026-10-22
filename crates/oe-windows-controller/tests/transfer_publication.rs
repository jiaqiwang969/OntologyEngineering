#![cfg(windows)]
use oe_nx_protocol::{segmented::TransferPlan, *};
use oe_windows_controller::{
    adapter, transfer_publication::*, transfer_reconstruct::Target,
    transfer_reconstruct_windows::PinnedTarget, windows::*,
};
use std::ffi::OsStr;
fn root_path() -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "oe-publication-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}
fn plan(root: oe_nx_protocol::RootIdentity) -> TransferPlan {
    decode(&serde_json::to_vec(&serde_json::json!({"schema":"oev2.nx-segmented-transfer/1","job_id":"source","root":root,"direction":"upload","source_manifest":Digest::of(b"source"),"budget":{"max_total_bytes":104857601,"max_chunk_bytes":3,"max_files":10,"max_chunks":10},"files":[FileEntry::from_bytes("part.prt",b"ABCD").unwrap()],"spans":[{"file_index":0,"offset":0,"bytes":3},{"file_index":0,"offset":3,"bytes":1}]})).unwrap()).unwrap()
}
#[test]
fn publication_revalidates_tree_and_repeats_without_rewriting() {
    let path = root_path();
    std::fs::create_dir(&path).unwrap();
    {
        let root = StableRoot::open(&path, None).unwrap();
        let lock = root.lock().unwrap();
        let out = root.create_directory_new(OsStr::new("output")).unwrap();
        out.create_file_new(OsStr::new("part.prt"), b"ABCD", 16)
            .unwrap();
        let expected = PinnedTarget::new(&out, &lock).unwrap().identity().unwrap();
        let p = plan(adapter::root_identity(&root));
        let output = JobId::new("output").unwrap();
        let publication = JobId::new("published").unwrap();
        let first = publish(
            &root,
            &lock,
            &p,
            &output,
            &publication,
            &expected,
            Operation::Publish,
        )
        .unwrap();
        assert_eq!(first.state, "published");
        assert!(!first.native_execution_started);
        let mut calls = 0;
        let again = publish_with_checkpoint(
            &root,
            &lock,
            &p,
            &output,
            &publication,
            &expected,
            Operation::Publish,
            |_| {
                calls += 1;
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(again, first);
        assert_eq!(calls, 0);
        std::fs::write(out.path().join("part.prt"), b"WXYZ").unwrap();
        assert!(
            publish(
                &root,
                &lock,
                &p,
                &output,
                &publication,
                &expected,
                Operation::Inspect
            )
            .is_err()
        );
        std::fs::write(out.path().join("part.prt"), b"ABCD").unwrap();
        let mut wrong = expected.clone();
        wrong.directory_id = "wrong".into();
        let next = JobId::new("must-not-exist").unwrap();
        assert!(publish(&root, &lock, &p, &output, &next, &wrong, Operation::Publish).is_err());
        assert!(!path.join(next.as_str()).exists());
        let published = root.directory(OsStr::new("published")).unwrap();
        published
            .create_file_new(OsStr::new("unexpected"), b"x", 16)
            .unwrap();
        assert!(
            publish(
                &root,
                &lock,
                &p,
                &output,
                &publication,
                &expected,
                Operation::Inspect
            )
            .is_err()
        );
    }
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn publication_pending_requires_explicit_recovery_at_each_atomic_checkpoint() {
    let path = root_path();
    std::fs::create_dir(&path).unwrap();
    {
        let root = StableRoot::open(&path, None).unwrap();
        let lock = root.lock().unwrap();
        let out = root.create_directory_new(OsStr::new("output")).unwrap();
        out.create_file_new(OsStr::new("part.prt"), b"ABCD", 16)
            .unwrap();
        let expected = PinnedTarget::new(&out, &lock).unwrap().identity().unwrap();
        let p = plan(adapter::root_identity(&root));
        let output = JobId::new("output").unwrap();
        for (i, stop) in [
            WriteCheckpoint::PendingSynced,
            WriteCheckpoint::TemporarySynced,
            WriteCheckpoint::ReplacedAndReadback,
        ]
        .into_iter()
        .enumerate()
        {
            let id = JobId::new(format!("interrupted-{i}")).unwrap();
            assert!(
                publish_with_checkpoint(
                    &root,
                    &lock,
                    &p,
                    &output,
                    &id,
                    &expected,
                    Operation::Publish,
                    |at| if at == stop {
                        Err(std::io::Error::other("injected interruption"))
                    } else {
                        Ok(())
                    }
                )
                .is_err()
            );
            assert!(
                publish(
                    &root,
                    &lock,
                    &p,
                    &output,
                    &id,
                    &expected,
                    Operation::Inspect
                )
                .is_err()
            );
            assert!(
                publish(
                    &root,
                    &lock,
                    &p,
                    &output,
                    &id,
                    &expected,
                    Operation::Publish
                )
                .is_err()
            );
            let recovered = publish(
                &root,
                &lock,
                &p,
                &output,
                &id,
                &expected,
                Operation::Recover,
            )
            .unwrap();
            assert_eq!(
                publish(
                    &root,
                    &lock,
                    &p,
                    &output,
                    &id,
                    &expected,
                    Operation::Inspect
                )
                .unwrap(),
                recovered
            );
            assert!(!path.join(id.as_str()).join(PENDING_NAME).exists());
            println!("publication_checkpoint_{i}=explicitly_recovered");
        }
        let id = JobId::new("partial-pending").unwrap();
        assert!(
            publish_with_checkpoint(
                &root,
                &lock,
                &p,
                &output,
                &id,
                &expected,
                Operation::Publish,
                |_| Err(std::io::Error::other("stop"))
            )
            .is_err()
        );
        let pending = path.join(id.as_str()).join(PENDING_NAME);
        std::fs::write(&pending, b"partial").unwrap();
        assert!(
            publish(
                &root,
                &lock,
                &p,
                &output,
                &id,
                &expected,
                Operation::Recover
            )
            .is_err()
        );
        assert_eq!(std::fs::read(pending).unwrap(), b"partial");
    }
    std::fs::remove_dir_all(path).unwrap();
}
