use oe_nx_protocol::{transfer::*, *};
fn put(data: &[u8]) -> Request {
    Request::TransferPut {
        job_id: JobId::new("transfer-test").unwrap(),
        plan_digest: Digest::of(b"plan"),
        index: 0,
        payload: BlobIdentity::of(data),
    }
}
#[test]
fn transfer_frames_bind_payload_without_relaxing_other_commands() {
    for data in [b"abc".as_slice(), b"".as_slice()] {
        let r = put(data);
        let mut bytes = Vec::new();
        write_request(&mut bytes, &r, data).unwrap();
        let (got, body) = read_request(&mut bytes.as_slice()).unwrap();
        assert_eq!(got, r);
        assert_eq!(body, data);
    }
    let r = put(b"abc");
    assert!(write_request(&mut Vec::new(), &r, b"xyz").is_err());
    assert!(write_request(&mut Vec::new(), &Request::Probe {}, b"abc").is_err());
    let mut bytes = Vec::new();
    write_request(&mut bytes, &r, b"abc").unwrap();
    *bytes.last_mut().unwrap() = b'x';
    assert!(read_request(&mut bytes.as_slice()).is_err());
}
#[test]
fn transfer_response_rejects_wrong_binding_or_execution_claim() {
    let request = put(b"abc");
    let (job_id, plan_digest) = binding(&request).unwrap();
    let root = RootIdentity {
        host_id: "h".into(),
        volume_id: "v".into(),
        directory_id: "d".into(),
    };
    let r = TransferResponse {
        schema: RESPONSE_SCHEMA.into(),
        root: root.clone(),
        job_id,
        plan_digest,
        next_index: 1,
        total_chunks: 2,
        status: "ready".into(),
        verified_bytes: None,
        native_execution_started: false,
        publication: None,
        reconstructed: None,
        reconstruction_output: None,
    };
    r.validate(&request, &root).unwrap();
    for n in 0..5 {
        let mut x = r.clone();
        match n {
            0 => x.next_index = 0,
            1 => x.root.directory_id = "other".into(),
            2 => x.native_execution_started = true,
            3 => x.status = "verified_chunks".into(),
            _ => x.plan_digest = Digest::of(b"different"),
        };
        assert!(x.validate(&request, &root).is_err());
    }
    assert!(
        validate_request(&Request::Run {
            job_id: r.job_id,
            manifest_digest: r.plan_digest,
            wait_seconds: 1
        })
        .is_err()
    );
}
#[test]
fn batch_frames_and_response_bounds() {
    let request = Request::TransferPutBatch {
        job_id: JobId::new("batch-test").unwrap(),
        plan_digest: Digest::of(b"plan"),
        index: 4,
        count: 3,
        payload: BlobIdentity::of(b"abc"),
    };
    let mut bytes = Vec::new();
    write_request(&mut bytes, &request, b"abc").unwrap();
    assert_eq!(
        read_request(&mut bytes.as_slice()).unwrap(),
        (request.clone(), b"abc".to_vec())
    );
    assert!(write_request(&mut Vec::new(), &request, b"xyz").is_err());
    for (index, count) in [(0, 0), (0, 4097), (65535, 2), (u32::MAX, 2)] {
        let mut bad = request.clone();
        if let Request::TransferPutBatch {
            index: i, count: c, ..
        } = &mut bad
        {
            *i = index;
            *c = count;
        }
        assert!(bad.validate().is_err());
    }
    let root = RootIdentity {
        host_id: "h".into(),
        volume_id: "v".into(),
        directory_id: "d".into(),
    };
    let (job_id, plan_digest) = binding(&request).unwrap();
    let mut response = TransferResponse {
        schema: RESPONSE_SCHEMA.into(),
        root: root.clone(),
        job_id,
        plan_digest,
        next_index: 7,
        total_chunks: 10,
        status: "ready".into(),
        verified_bytes: None,
        native_execution_started: false,
        publication: None,
        reconstructed: None,
        reconstruction_output: None,
    };
    response.validate(&request, &root).unwrap();
    response.next_index = 6;
    assert!(response.validate(&request, &root).is_err());
}
#[test]
fn reconstruction_response_never_grants_publication_or_execution() {
    let request = Request::TransferReconstruct {
        job_id: JobId::new("source").unwrap(),
        plan_digest: Digest::of(b"plan"),
        output_id: JobId::new("output").unwrap(),
    };
    let (job_id, plan_digest) = binding(&request).unwrap();
    let root = RootIdentity {
        host_id: "h".into(),
        volume_id: "v".into(),
        directory_id: "root".into(),
    };
    let mut target = root.clone();
    target.directory_id = "new-tree".into();
    let tree = ReconstructedTree {
        schema: "oev2.nx-reconstructed-tree/1".into(),
        plan_digest: plan_digest.clone(),
        target,
        inventory_digest: Digest::of(b"files"),
        files: 1,
        bytes: 4,
        published: false,
        native_execution_started: false,
    };
    let response = TransferResponse {
        schema: RESPONSE_SCHEMA.into(),
        root: root.clone(),
        job_id,
        plan_digest,
        next_index: 2,
        total_chunks: 2,
        status: "reconstructed_files".into(),
        verified_bytes: Some(4),
        native_execution_started: false,
        publication: None,
        reconstructed: Some(tree),
        reconstruction_output: Some(JobId::new("output").unwrap()),
    };
    response.validate(&request, &root).unwrap();
    for mode in 0..6 {
        let mut r = response.clone();
        match mode {
            0 => r.reconstruction_output = Some(JobId::new("wrong").unwrap()),
            1 => r.reconstructed.as_mut().unwrap().published = true,
            2 => r.reconstructed.as_mut().unwrap().native_execution_started = true,
            3 => r.reconstructed.as_mut().unwrap().target = root.clone(),
            4 => r.verified_bytes = Some(5),
            _ => r.next_index = 1,
        };
        assert!(r.validate(&request, &root).is_err());
    }
}
#[test]
fn publication_response_binds_physical_tree_and_never_grants_native() {
    let root = RootIdentity {
        host_id: "h".into(),
        volume_id: "v".into(),
        directory_id: "root".into(),
    };
    let mut target = root.clone();
    target.directory_id = "tree".into();
    let mut publication_root = root.clone();
    publication_root.directory_id = "publication".into();
    let job_id = JobId::new("source").unwrap();
    let output_id = JobId::new("output").unwrap();
    let publication_id = JobId::new("publication").unwrap();
    let plan_digest = Digest::of(b"plan");
    let request = Request::TransferPublish {
        job_id: job_id.clone(),
        plan_digest: plan_digest.clone(),
        output_id: output_id.clone(),
        publication_id: publication_id.clone(),
        expected_target: target.clone(),
    };
    let publication = SourcePublication {
        schema: "oev2.nx-source-publication/1".into(),
        source_root: root.clone(),
        job_id: job_id.clone(),
        plan_digest: plan_digest.clone(),
        output_id,
        publication_id,
        publication_root,
        target,
        inventory_digest: Digest::of(b"files"),
        files: 1,
        bytes: 4,
        state: "published".into(),
        native_execution_started: false,
    };
    let response = TransferResponse {
        schema: RESPONSE_SCHEMA.into(),
        root: root.clone(),
        job_id,
        plan_digest,
        next_index: 2,
        total_chunks: 2,
        status: "published_source".into(),
        verified_bytes: Some(4),
        native_execution_started: false,
        publication: Some(publication),
        reconstructed: None,
        reconstruction_output: None,
    };
    response.validate(&request, &root).unwrap();
    for mode in 0..6 {
        let mut r = response.clone();
        let p = r.publication.as_mut().unwrap();
        match mode {
            0 => p.target.directory_id = "other".into(),
            1 => p.publication_root = root.clone(),
            2 => p.native_execution_started = true,
            3 => p.state = "pending".into(),
            4 => p.bytes = 5,
            _ => p.publication_id = JobId::new("wrong").unwrap(),
        };
        assert!(r.validate(&request, &root).is_err());
    }
}
