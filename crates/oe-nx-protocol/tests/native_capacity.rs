use oe_nx_protocol::*;
use serde_json::json;
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

fn capacity() -> NativeCapacity {
    serde_json::from_value(json!({"max_input_bytes":268435456,"max_input_file_bytes":209715200,"max_output_bytes":536870912,"max_output_file_bytes":268435456,"max_input_files":10000,"max_output_files":10000,"max_segment_bytes":104857600})).unwrap()
}
fn large() -> JobManifest {
    let mut j = job();
    j.schema = LARGE_JOB_SCHEMA.into();
    j.capacity = Some(capacity());
    j
}
fn auxiliary() -> NativeAuxiliaryCapacity {
    serde_json::from_value(
        json!({"max_total_bytes":536870912,"max_file_bytes":67108864,"max_files":8192}),
    )
    .unwrap()
}
#[test]
fn auxiliary_capacity_is_explicit_versioned_and_identity_bound() {
    for j in [job(), large()] {
        let raw = serde_json::to_value(&j).unwrap();
        assert!(raw.get("auxiliary_capacity").is_none());
        assert_eq!(
            decode::<JobManifest>(&serde_json::to_vec(&raw).unwrap()).unwrap(),
            j
        );
        assert_eq!(j.digest().unwrap(), record_digest(&raw).unwrap());
        assert_eq!(
            j.auxiliary_bounds().unwrap(),
            (
                large_fetch::MAX_AUX_BYTES,
                large_fetch::MAX_AUX_FILE_BYTES,
                1088
            )
        );
    }
    let mut j = large();
    let before = j.digest().unwrap();
    j.auxiliary_capacity = Some(auxiliary());
    assert_ne!(j.digest().unwrap(), before);
    let bound = j.digest().unwrap();
    assert_eq!(j.auxiliary_bounds().unwrap(), (536870912, 67108864, 8192));
    j.auxiliary_capacity.as_mut().unwrap().0.max_files += 1;
    assert_ne!(j.digest().unwrap(), bound);
    let mut old = job();
    old.auxiliary_capacity = Some(auxiliary());
    assert!(old.validate().is_err());
    for (field, value) in [
        ("max_files", 0),
        ("max_files", 65537),
        ("max_total_bytes", 0),
        ("max_total_bytes", 536870913),
        ("max_file_bytes", 0),
        ("max_file_bytes", 67108865),
    ] {
        let mut raw = serde_json::to_value(auxiliary()).unwrap();
        raw[field] = json!(value);
        let mut bad = large();
        bad.auxiliary_capacity = Some(serde_json::from_value(raw).unwrap());
        assert!(bad.validate().is_err(), "{field}/{value}");
    }
    let mut bad = large();
    let mut cap = auxiliary();
    cap.0.max_total_bytes = 1;
    bad.auxiliary_capacity = Some(cap);
    assert!(bad.validate().is_err());
}
#[test]
fn native_capacity_preserves_legacy_and_binds_versions() {
    let old = job();
    let raw = serde_json::to_value(&old).unwrap();
    assert!(raw.get("capacity").is_none());
    assert_eq!(
        decode::<JobManifest>(&serde_json::to_vec(&old).unwrap()).unwrap(),
        old
    );
    let mut j = old.clone();
    j.inputs[0].identity.bytes = 150 * 1024 * 1024;
    assert!(j.validate().is_err());
    j.schema = LARGE_JOB_SCHEMA.into();
    assert!(j.validate().is_err());
    j.capacity = Some(capacity());
    j.validate().unwrap();
    j.input_digest().unwrap();
    j.verify_payload(&j.payload_files()).unwrap();
    let digest = j.digest().unwrap();
    j.capacity.as_mut().unwrap().0.max_output_bytes += 1;
    assert_ne!(j.digest().unwrap(), digest);
    j.schema = JOB_SCHEMA.into();
    assert!(j.validate().is_err());
    j.schema = "oev2.nx-native-job/4".into();
    assert!(j.validate().is_err());
    let mut missing = serde_json::to_value(capacity()).unwrap();
    missing.as_object_mut().unwrap().remove("max_segment_bytes");
    assert!(serde_json::from_value::<NativeCapacity>(missing).is_err());
}
#[test]
fn native_capacity_rejects_overflow_aliases_and_output_excess() {
    let mut j = large();
    j.inputs[0].identity.bytes = u64::MAX;
    assert!(j.validate().is_err());
    j = large();
    j.capacity.as_mut().unwrap().0.max_input_bytes = u64::MAX;
    assert!(j.validate().is_err());
    j = large();
    j.capacity.as_mut().unwrap().0.max_segment_bytes = MAX_PAYLOAD_BYTES + 1;
    assert!(j.validate().is_err());
    j = large();
    j.capacity.as_mut().unwrap().0.max_input_files = 1;
    assert!(j.validate().is_err());
    j = large();
    j.inputs = vec![file("inputs/A.prt", b"a"), file("inputs/a.PRT", b"b")];
    assert!(j.validate().is_err());
    j.inputs = vec![file("inputs/A", b"a"), file("inputs/a/x", b"b")];
    assert!(j.validate().is_err());
    j = large();
    let mut out = file("out/full.prt", b"x");
    out.identity.bytes = 150 * 1024 * 1024;
    j.verify_artifacts(&[out.clone()], &[out.clone()]).unwrap();
    assert!(
        job()
            .verify_artifacts(&[out.clone()], &[out.clone()])
            .is_err()
    );
    let mut wrong = out.clone();
    wrong.identity.sha256 = Digest::of(b"different");
    assert!(j.verify_artifacts(&[out.clone()], &[wrong]).is_err());
    out.identity.bytes = 268435457;
    assert!(j.verify_artifacts(&[out.clone()], &[out]).is_err());
    let entries = (0..3)
        .map(|i| {
            let mut f = file(&format!("out/{i}"), b"x");
            f.identity.bytes = 200 * 1024 * 1024;
            f
        })
        .collect::<Vec<_>>();
    assert!(j.verify_artifacts(&entries, &entries).is_err());
}
#[test]
fn native_capacity_blocks_legacy_archive_admission() {
    let j = large();
    j.validate().unwrap();
    assert!(
        Request::Stage {
            job: Box::new(j.clone()),
            archive: BlobIdentity::of(b"zip")
        }
        .validate()
        .is_err()
    );
    let p = Package {
        job: j,
        members: Default::default(),
    };
    assert!(p.verify().is_err());
    assert!(
        Request::Stage {
            job: Box::new(job()),
            archive: BlobIdentity::of(b"zip")
        }
        .validate()
        .is_ok()
    );
}

#[test]
fn inventory_bounds_preserve_versions_and_direction() {
    let old = job();
    assert_eq!(
        old.inventory_bounds(false).unwrap(),
        (MAX_PAYLOAD_BYTES, MAX_PAYLOAD_BYTES, MAX_FILES)
    );
    let j = large();
    assert_eq!(
        j.inventory_bounds(false).unwrap(),
        (268435456, 209715200, 10000)
    );
    assert_eq!(
        j.inventory_bounds(true).unwrap(),
        (536870912, 268435456, 10000)
    );
    let mut wrong = j.clone();
    wrong.schema = JOB_SCHEMA.into();
    assert!(wrong.inventory_bounds(false).is_err());
    let mut wrong = j;
    wrong.capacity.as_mut().unwrap().0.max_output_file_bytes = 0;
    assert!(wrong.inventory_bounds(true).is_err());
}
