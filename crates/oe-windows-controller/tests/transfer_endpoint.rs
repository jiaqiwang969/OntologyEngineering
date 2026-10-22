#![cfg(windows)]
use oe_nx_protocol::{segmented::TransferPlan, *};
use oe_windows_controller::{
    adapter,
    transfer_endpoint::{ensure_native_root, execute},
    windows::StableRoot,
};
fn plan(root: RootIdentity) -> TransferPlan {
    decode(&serde_json::to_vec(&serde_json::json!({"schema":"oev2.nx-segmented-transfer/1","job_id":"transfer-endpoint-01","root":root,"direction":"upload","source_manifest":Digest::of(b"source"),"budget":{"max_total_bytes":104857601,"max_chunk_bytes":3,"max_files":10,"max_chunks":10},"files":[FileEntry::from_bytes("inputs/a",b"abcd").unwrap()],"spans":[{"file_index":0,"offset":0,"bytes":3},{"file_index":0,"offset":3,"bytes":1}]})).unwrap()).unwrap()
}
#[test]
fn endpoint_keeps_transfer_and_native_admission_separate() {
    let path = std::env::temp_dir().join(format!(
        "oe-transfer-endpoint-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&path).unwrap();
    {
        let root = StableRoot::open(&path, None).unwrap();
        let p = plan(adapter::root_identity(&root));
        let digest = p.binding().unwrap();
        let begin = Request::TransferBegin {
            plan_record: serde_json::to_string(&p).unwrap(),
        };
        assert_eq!(execute(&root, &begin, b"").unwrap().next_index, 0);
        assert!(ensure_native_root(&root).is_err());
        assert!(execute(&root, &begin, b"").is_err());
        for (i, data) in [b"abc".as_slice(), b"d".as_slice()].iter().enumerate() {
            let put = Request::TransferPut {
                job_id: p.job_id.clone(),
                plan_digest: digest.clone(),
                index: i as u32,
                payload: BlobIdentity::of(data),
            };
            assert_eq!(execute(&root, &put, data).unwrap().next_index, i as u32 + 1);
            assert!(execute(&root, &put, data).is_err());
        }
        let inspect = Request::TransferInspect {
            job_id: p.job_id.clone(),
            plan_digest: digest.clone(),
        };
        assert_eq!(execute(&root, &inspect, b"").unwrap().next_index, 2);
        let rebuild = Request::TransferReconstruct {
            job_id: p.job_id.clone(),
            plan_digest: digest.clone(),
            output_id: JobId::new("complete-files").unwrap(),
        };
        let r = execute(&root, &rebuild, b"").unwrap();
        assert_eq!(r.status, "reconstructed_files");
        let tree = r.reconstructed.unwrap();
        assert_eq!(tree.bytes, 4);
        assert_eq!(
            std::fs::read(path.join("complete-files/inputs/a")).unwrap(),
            b"abcd"
        );
        assert!(execute(&root, &rebuild, b"").is_err());
        let publication = Request::TransferPublish {
            job_id: p.job_id.clone(),
            plan_digest: digest.clone(),
            output_id: JobId::new("complete-files").unwrap(),
            publication_id: JobId::new("published-files").unwrap(),
            expected_target: tree.target,
        };
        let published = execute(&root, &publication, b"").unwrap();
        assert_eq!(published.status, "published_source");
        assert_eq!(published.publication.unwrap().bytes, 4);
        assert!(ensure_native_root(&root).is_err());
        execute(&root, &publication, b"").unwrap();
        let verify = Request::TransferVerify {
            job_id: p.job_id,
            plan_digest: digest,
        };
        assert_eq!(
            execute(&root, &verify, b"").unwrap().verified_bytes,
            Some(4)
        );
    }
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn transfer_refuses_occupied_native_root_without_creating_marker() {
    let path = std::env::temp_dir().join(format!(
        "oe-transfer-occupied-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&path).unwrap();
    std::fs::create_dir(path.join("old-native-job")).unwrap();
    {
        let root = StableRoot::open(&path, None).unwrap();
        let p = plan(adapter::root_identity(&root));
        assert!(
            execute(
                &root,
                &Request::TransferBegin {
                    plan_record: serde_json::to_string(&p).unwrap()
                },
                b""
            )
            .is_err()
        );
        assert!(!path.join(".oe-transfer-root.json").exists());
        ensure_native_root(&root).unwrap();
    }
    std::fs::remove_dir_all(path).unwrap();
}
