use oe_contracts::NxTransferBudget;
use oe_nx_protocol::{segmented::*, *};
fn budget() -> NxTransferBudget {
    NxTransferBudget {
        max_total_bytes: 256 * 1024 * 1024,
        max_chunk_bytes: 3,
        max_files: 10000,
        max_chunks: 65536,
    }
}
fn plan() -> TransferPlan {
    TransferPlan::new(
        JobId::new("segmented-fixture-01").unwrap(),
        RootIdentity {
            host_id: "test-host".into(),
            volume_id: "v1".into(),
            directory_id: "d1".into(),
        },
        "upload".into(),
        Digest::of(b"frozen manifest"),
        budget(),
        vec![
            FileEntry::from_bytes("inputs/b", b"abcd").unwrap(),
            FileEntry::from_bytes("inputs/a", b"").unwrap(),
        ],
    )
    .unwrap()
}
#[test]
fn complete_chunks_preserve_files_and_binding() {
    let p = plan();
    assert_eq!(
        p.spans,
        vec![
            Span {
                file_index: 0,
                offset: 0,
                bytes: 0
            },
            Span {
                file_index: 1,
                offset: 0,
                bytes: 3
            },
            Span {
                file_index: 1,
                offset: 3,
                bytes: 1
            }
        ]
    );
    let id = p.binding().unwrap();
    let mut r = Receiver::new(p).unwrap();
    r.accept(&id, 0, b"").unwrap();
    r.accept(&id, 1, b"abc").unwrap();
    r.accept(&id, 2, b"d").unwrap();
    let v = r.finish().unwrap();
    assert_eq!((v.files, v.bytes), (2, 4));
    assert_eq!(v.plan_digest, id);
}
#[test]
fn corruption_gaps_and_cross_task_binding_are_terminal() {
    let p = plan();
    let id = p.binding().unwrap();
    for fault in 0..6 {
        let mut r = Receiver::new(p.clone()).unwrap();
        match fault {
            0 => {
                assert!(r.accept(&id, 1, b"abc").is_err());
            }
            1 => {
                let mut wrong = p.clone();
                wrong.root.directory_id = "different".into();
                assert!(r.accept(&wrong.binding().unwrap(), 0, b"").is_err());
            }
            2 => {
                r.accept(&id, 0, b"").unwrap();
                assert!(r.accept(&id, 0, b"").is_err());
            }
            3 => {
                r.accept(&id, 0, b"").unwrap();
                r.accept(&id, 1, b"abX").unwrap();
                assert!(r.accept(&id, 2, b"d").is_err());
            }
            4 => {
                assert!(r.accept(&id, 0, b"x").is_err());
            }
            _ => {
                let mut wrong = p.clone();
                wrong.job_id = JobId::new("different-job").unwrap();
                assert!(r.accept(&wrong.binding().unwrap(), 0, b"").is_err());
            }
        }
        assert!(r.accept(&id, 0, b"").is_err());
        assert!(r.finish().is_err());
    }
    let mut r = Receiver::new(p).unwrap();
    r.accept(&id, 0, b"").unwrap();
    assert!(r.finish().is_err());
}
#[test]
fn legacy_limit_and_new_budget_are_separate() {
    let mut p = plan();
    p.files = vec![FileEntry {
        path: RelativePath::new("inputs/large").unwrap(),
        identity: BlobIdentity {
            bytes: MAX_PAYLOAD_BYTES + 1,
            sha256: Digest::of(b"metadata fixture only"),
        },
    }];
    assert_eq!(
        validate_inventory(&p.files).unwrap_err().to_string(),
        "payload exceeds bound"
    );
    p.budget.max_chunk_bytes = MAX_PAYLOAD_BYTES;
    let mut p = TransferPlan::new(
        p.job_id,
        p.root,
        p.direction,
        p.source_manifest,
        p.budget,
        p.files,
    )
    .unwrap();
    assert_eq!(p.spans.len(), 2);
    assert_eq!(p.spans[1].bytes, 1);
    p.validate().unwrap();
    p.budget.max_chunk_bytes = MAX_PAYLOAD_BYTES + 1;
    assert!(p.validate().is_err());
}
#[test]
fn aliases_and_manifest_tampering_are_rejected() {
    let mut p = plan();
    p.spans[2].offset = 2;
    assert!(p.validate().is_err());
    let mut p = plan();
    p.spans.pop();
    assert!(p.validate().is_err());
    let mut p = plan();
    p.budget.max_chunks = 2;
    assert!(p.validate().is_err());
    let p = plan();
    let mut files = p.files;
    files.push(FileEntry::from_bytes("inputs/A", b"x").unwrap());
    assert!(
        TransferPlan::new(
            p.job_id,
            p.root,
            p.direction,
            p.source_manifest,
            p.budget,
            files
        )
        .is_err()
    );
    let mut p = plan();
    p.files[0].identity.sha256 = Digest::of(b"nonempty");
    let id = p.binding().unwrap();
    let mut r = Receiver::new(p).unwrap();
    assert!(r.accept(&id, 0, b"").is_err());
    assert!(r.finish().is_err());
}
