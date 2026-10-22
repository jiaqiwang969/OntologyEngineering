use oe_cad::nx::*;
use oe_core::SkillRoot;
use oe_nx_protocol::*;
use std::{
    fs,
    io::{Cursor, Write},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use zip::{ZipWriter, write::SimpleFileOptions};

struct Fixture {
    directory: PathBuf,
    root: SkillRoot,
    request: PrepareRequest,
}
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("var/tests/nx")
            .join(format!(
                "package-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(directory.parent().unwrap()).unwrap();
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join("SKILL.md"), "test-only source boundary").unwrap();
        fs::create_dir(directory.join("var")).unwrap();
        let worker = directory.join("var/worker-package");
        fs::create_dir(&worker).unwrap();
        let exe =
            FileEntry::from_bytes("worker/native.exe", b"MZfixture-only-never-executed").unwrap();
        let source =
            FileEntry::from_bytes("sources/main.rs", b"// fixture, not compiled evidence").unwrap();
        let build = FileEntry::from_bytes("build/recipe.txt", b"fixture, not a real toolchain run")
            .unwrap();
        for (file, bytes) in [
            (&exe, &b"MZfixture-only-never-executed"[..]),
            (&source, &b"// fixture, not compiled evidence"[..]),
            (&build, &b"fixture, not a real toolchain run"[..]),
        ] {
            let path = worker.join(file.path.as_str());
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
        }
        let manifest = WorkerManifest {
            schema: WORKER_SCHEMA.into(),
            worker_id: "synthetic-native".into(),
            version: "test".into(),
            target: "x86_64-pc-windows-gnu".into(),
            executable: exe,
            runtime_files: vec![],
            source_files: vec![source],
            build_recipe: build,
            toolchain: "synthetic-no-compiler".into(),
            required_runtime: RuntimeIdentity {
                product: "NX".into(),
                product_label: "mixed-per-library-versions".into(),
                architecture: "x64".into(),
                libraries: vec![
                    FileEntry::from_bytes("NXBIN/libufun.dll", b"synthetic-identity-only").unwrap(),
                ],
            },
            capabilities: vec![Capability {
                action: "inspect".into(),
                params_schema: "fixture.inspect/v1".into(),
            }],
        };
        fs::write(
            worker.join("worker.json"),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        let params = directory.join("var/parameters.json");
        fs::write(
            &params,
            br#"{"schema":"fixture.inspect/v1","parts":["part.prt"]}"#,
        )
        .unwrap();
        let input = directory.join("var/part.prt");
        fs::write(&input, b"synthetic-source-part").unwrap();
        let request = PrepareRequest {
            worker_directory: worker,
            parameter_file: params,
            inputs: vec![InputBinding {
                source: input,
                member: RelativePath::new("inputs/part.prt").unwrap(),
            }],
            job_id: JobId::new("package-test").unwrap(),
            root_identity: RootIdentity {
                host_id: "fixture-host".into(),
                volume_id: "fixture-volume".into(),
                directory_id: "fixture-directory".into(),
            },
            action: "inspect".into(),
            parameter_schema: "fixture.inspect/v1".into(),
        };
        let root = SkillRoot::new(&directory).unwrap();
        Self {
            directory,
            root,
            request,
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.directory).unwrap();
    }
}

#[test]
fn assembly_parameters_round_trip_to_actual_worker_nested_job_input() {
    for member in [
        "inputs/reference/payload/cad/ap203/files/model/assembly.STEP",
        "inputs/reference/payload/cad/ap214/files/model/assembly.stp",
    ] {
        let mut f = Fixture::new();
        let budget = oe_contracts::NxAssemblyBudget {
            max_occurrences: 100,
            max_prototypes: 20,
            max_depth: 10,
        };
        let raw = import_assembly_parameters(member, &budget).unwrap();
        let parsed: oe_nx_worker::geometry::Parameters = decode(&raw).unwrap();
        parsed.validate().unwrap();
        let oe_nx_worker::geometry::Parameters::ImportStepAssembly { step, .. } = parsed else {
            panic!("existing worker import action required");
        };
        let worker_lookup = oe_nx_worker::geometry::input_relative(&step).unwrap();
        assert_eq!(worker_lookup.as_str(), member);
        let worker_manifest_path = f.request.worker_directory.join("worker.json");
        let mut worker: WorkerManifest = decode(&fs::read(&worker_manifest_path).unwrap()).unwrap();
        worker.capabilities.push(Capability {
            action: "import_step_assembly".into(),
            params_schema: oe_nx_worker::assembly::PARAMS_SCHEMA.into(),
        });
        fs::write(&worker_manifest_path, serde_json::to_vec(&worker).unwrap()).unwrap();
        fs::write(&f.request.parameter_file, &raw).unwrap();
        f.request.inputs[0].member = RelativePath::new(member).unwrap();
        f.request.action = "import_step_assembly".into();
        f.request.parameter_schema = oe_nx_worker::assembly::PARAMS_SCHEMA.into();
        let package = prepare(&f.root, &f.request).unwrap();
        let actual_entry = package
            .job
            .inputs
            .iter()
            .find(|entry| entry.path == worker_lookup)
            .expect("actual worker lookup must address a declared job input");
        assert_eq!(
            package.members[actual_entry.path.as_str()],
            fs::read(&f.request.inputs[0].source).unwrap()
        );
    }
}

#[test]
fn assembly_job_member_conversion_preserves_namespace_and_worker_guards() {
    let budget = oe_contracts::NxAssemblyBudget {
        max_occurrences: 100,
        max_prototypes: 20,
        max_depth: 10,
    };
    for bad in [
        "reference/model.step",
        "worker/model.step",
        "inputs/",
        "/inputs/model.step",
        "C:/inputs/model.step",
        "inputs/../model.step",
        "inputs/reference/../../model.step",
        "inputs//model.step",
        "inputs/reference\\model.step",
        "inputs/reference/model.prt",
        "inputs/reference/model.step.txt",
        "inputs/reference/零件.step",
    ] {
        assert!(import_assembly_parameters(bad, &budget).is_err(), "{bad}");
    }
    let invalid_budget = oe_contracts::NxAssemblyBudget {
        max_occurrences: 0,
        ..budget
    };
    assert!(import_assembly_parameters("inputs/reference/model.step", &invalid_budget).is_err());
}

#[test]
fn complete_source_package_round_trip_and_new_private_publication() {
    let f = Fixture::new();
    let p = prepare(&f.root, &f.request).unwrap();
    let archive = p.archive().unwrap();
    let recovered = from_archive(&archive).unwrap();
    assert_eq!(p.job, recovered.job);
    assert_eq!(p.members, recovered.members);
    let out = publish(&p, &f.root, &PathBuf::from("var/prepared")).unwrap();
    assert_eq!(
        from_directory(&f.root, &out.join("job")).unwrap().members,
        p.members
    );
    assert_eq!(fs::read(out.join("job.zip")).unwrap(), archive);
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(out.join("prepared.json")).unwrap()).unwrap();
    assert_eq!(report["native_execution"], "not_run");
    assert!(publish(&p, &f.root, &PathBuf::from("var/prepared")).is_err());
    assert!(publish(&p, &f.root, &PathBuf::from("prepared-outside-var")).is_err());
}

#[test]
fn changed_or_extra_worker_sources_and_unknown_actions_block_before_publication() {
    let mut f = Fixture::new();
    let path = f.request.worker_directory.join("sources/main.rs");
    let before = fs::read(&path).unwrap();
    fs::write(&path, b"changed").unwrap();
    assert!(prepare(&f.root, &f.request).is_err());
    fs::write(&path, before).unwrap();
    let extra = f.request.worker_directory.join("unaccounted.txt");
    fs::write(&extra, b"extra").unwrap();
    assert!(prepare(&f.root, &f.request).is_err());
    fs::remove_file(extra).unwrap();
    f.request.action = "unknown".into();
    assert!(prepare(&f.root, &f.request).is_err());
    assert!(!f.directory.join("var/prepared").exists());
}

#[test]
fn parameter_schema_and_duplicate_json_keys_are_never_normalized_away() {
    let f = Fixture::new();
    for bytes in [
        br#"{"schema":"other/v1"}"#.as_slice(),
        br#"{"schema":"fixture.inspect/v1","x":1,"x":2}"#.as_slice(),
    ] {
        fs::write(&f.request.parameter_file, bytes).unwrap();
        assert!(prepare(&f.root, &f.request).is_err());
    }
}

#[test]
fn original_unicode_part_name_survives_zip_and_preserves_parameter_reference() {
    let mut f = Fixture::new();
    f.request.inputs[0].member = RelativePath::new("inputs/装配_零件一.prt").unwrap();
    fs::write(
        &f.request.parameter_file,
        "{\"schema\":\"fixture.inspect/v1\",\"parts\":[\"装配_零件一.prt\"]}",
    )
    .unwrap();
    let p = prepare(&f.root, &f.request).unwrap();
    let restored = from_archive(&p.archive().unwrap()).unwrap();
    assert!(restored.members.contains_key("inputs/装配_零件一.prt"));
    assert_eq!(
        restored.members["params.json"],
        fs::read(&f.request.parameter_file).unwrap()
    );
}

fn zip_members(items: Vec<(String, Vec<u8>)>) -> Vec<u8> {
    let mut z = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in items {
        z.start_file(
            name,
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
        )
        .unwrap();
        z.write_all(&bytes).unwrap();
    }
    z.finish().unwrap().into_inner()
}

#[test]
fn zip_traversal_extra_members_and_changed_bytes_fail_complete_intake() {
    let f = Fixture::new();
    let p = prepare(&f.root, &f.request).unwrap();
    for name in [
        "../escape",
        "inputs/C:evil.prt",
        "inputs/PART.prt",
        "extra.txt",
    ] {
        let mut items = p
            .members
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect::<Vec<_>>();
        items.push((name.into(), b"extra".to_vec()));
        assert!(from_archive(&zip_members(items)).is_err());
    }
    let mut members = p.members.clone();
    members.get_mut("inputs/part.prt").unwrap().push(b'x');
    assert!(from_archive(&zip_members(members.into_iter().collect())).is_err());
}

#[test]
fn real_zip_crc_damage_rejected_before_destination_creation() {
    let f = Fixture::new();
    let p = prepare(&f.root, &f.request).unwrap();
    let mut bytes = zip_members(p.members.into_iter().collect());
    let needle = b"synthetic-source-part";
    let offset = bytes
        .windows(needle.len())
        .position(|x| x == needle)
        .unwrap();
    bytes[offset] ^= 1;
    assert!(from_archive(&bytes).is_err());
    assert!(!f.directory.join("var/prepared").exists());
}

#[cfg(unix)]
#[test]
fn links_and_outside_skill_inputs_cannot_be_staged() {
    use std::os::unix::fs::symlink;
    let mut f = Fixture::new();
    let link = f.directory.join("var/link.prt");
    symlink(&f.request.inputs[0].source, &link).unwrap();
    f.request.inputs[0].source = link;
    assert!(prepare(&f.root, &f.request).is_err());
    f.request.inputs[0].source = PathBuf::from("/etc/hosts");
    assert!(prepare(&f.root, &f.request).is_err());
}

#[test]
fn prepared_directory_added_file_or_modified_manifest_cannot_be_resent() {
    let f = Fixture::new();
    let p = prepare(&f.root, &f.request).unwrap();
    let out = publish(&p, &f.root, &PathBuf::from("var/prepared")).unwrap();
    fs::write(out.join("job/extra"), b"unaccounted").unwrap();
    assert!(from_directory(&f.root, &out.join("job")).is_err());
    fs::remove_file(out.join("job/extra")).unwrap();
    fs::write(out.join("job/job.json"), b"{}").unwrap();
    assert!(from_directory(&f.root, &out.join("job")).is_err());
}

#[test]
fn fetched_original_bytes_preserve_malformed_legacy_state_without_approving_it() {
    let f = Fixture::new();
    let package = prepare(&f.root, &f.request).unwrap();
    let mut members = package.members;
    let original_state = b"{malformed old state: evidence only\r\n";
    members.insert("state.json".into(), original_state.to_vec());
    members.insert("out/观察记录.txt".into(), "原始观察".as_bytes().to_vec());
    let archive = oe_nx_protocol::bundle::archive_members(&members).unwrap();
    let response = FetchResponse {
        schema: FETCH_RESPONSE_SCHEMA.into(),
        job_id: f.request.job_id.clone(),
        root: f.request.root_identity.clone(),
        archive: BlobIdentity::of(&archive),
        members: members
            .iter()
            .map(|(n, b)| FileEntry::from_bytes(n, b).unwrap())
            .collect(),
        state_file: Some(BlobIdentity::of(original_state)),
    };
    let output = client::publish_fetch(
        &f.root,
        &response,
        &f.request.job_id,
        &f.request.root_identity,
        &archive,
        &PathBuf::from("var/fetched"),
    )
    .unwrap();
    assert_eq!(
        fs::read(output.join("job/state.json")).unwrap(),
        original_state
    );
    assert_eq!(fs::read(output.join("job.zip")).unwrap(), archive);
    let complete: serde_json::Value =
        serde_json::from_slice(&fs::read(output.join("fetched.json")).unwrap()).unwrap();
    assert_eq!(complete["engineering_acceptance"], "not_evaluated");
    assert!(
        client::publish_fetch(
            &f.root,
            &response,
            &f.request.job_id,
            &f.request.root_identity,
            &archive,
            &PathBuf::from("var/fetched")
        )
        .is_err()
    );
    let mut changed = response;
    changed.state_file = Some(BlobIdentity::of(b"some other state"));
    assert!(
        client::publish_fetch(
            &f.root,
            &changed,
            &f.request.job_id,
            &f.request.root_identity,
            &archive,
            &PathBuf::from("var/rejected")
        )
        .is_err()
    );
    assert!(!f.directory.join("var/rejected").exists());
}

#[test]
fn fetched_empty_directories_require_archive_bound_controller_observation() {
    let f = Fixture::new();
    let mut members = std::collections::BTreeMap::from([(
        "state.json".into(),
        b"original legacy bytes".to_vec(),
    )]);
    let envelope = serde_json::json!({"schema":"oev2.nx-directory-envelope/1","job_id":f.request.job_id,"root":f.request.root_identity,
        "directories":["out","receipts"],"origin":"read_only_pinned_controller_observation","native_state_modified":false});
    members.insert(
        "oev2-directory-envelope.json".into(),
        serde_json::to_vec(&envelope).unwrap(),
    );
    let archive = oe_nx_protocol::bundle::archive_members(&members).unwrap();
    let response = FetchResponse {
        schema: FETCH_RESPONSE_SCHEMA.into(),
        job_id: f.request.job_id.clone(),
        root: f.request.root_identity.clone(),
        archive: BlobIdentity::of(&archive),
        members: members
            .iter()
            .map(|(n, b)| FileEntry::from_bytes(n, b).unwrap())
            .collect(),
        state_file: Some(BlobIdentity::of(&members["state.json"])),
    };
    let output = client::publish_fetch(
        &f.root,
        &response,
        &f.request.job_id,
        &f.request.root_identity,
        &archive,
        &PathBuf::from("var/empty-fetched"),
    )
    .unwrap();
    assert!(output.join("job/out").is_dir());
    assert_eq!(fs::read_dir(output.join("job/out")).unwrap().count(), 0);
    assert_eq!(
        fs::read(output.join("job/state.json")).unwrap(),
        b"original legacy bytes"
    );
    for directories in [
        serde_json::json!(["../escape"]),
        serde_json::json!(["state.json"]),
        serde_json::json!(["out", "OUT"]),
        serde_json::json!(["state.json/child"]),
    ] {
        let mut wrong = envelope.clone();
        wrong["directories"] = directories;
        members.insert(
            "oev2-directory-envelope.json".into(),
            serde_json::to_vec(&wrong).unwrap(),
        );
        assert!(
            client::validate_directory_envelope(
                &members,
                &f.request.job_id,
                &f.request.root_identity
            )
            .is_err()
        );
    }
    let mut wrong = envelope;
    wrong["job_id"] = serde_json::json!("other-job");
    members.insert(
        "oev2-directory-envelope.json".into(),
        serde_json::to_vec(&wrong).unwrap(),
    );
    assert!(
        client::validate_directory_envelope(&members, &f.request.job_id, &f.request.root_identity)
            .is_err()
    );
}

#[test]
fn worker_verification_binds_exact_bytes_without_job_or_runtime_claims() {
    let f = Fixture::new();
    let manifest_bytes = fs::read(f.request.worker_directory.join("worker.json")).unwrap();
    let manifest: WorkerManifest = decode(&manifest_bytes).unwrap();
    // These records are required by prepare, but irrelevant to package inspection.
    fs::remove_file(&f.request.parameter_file).unwrap();
    fs::remove_file(&f.request.inputs[0].source).unwrap();
    let report = verify_worker(&f.root, &f.request.worker_directory).unwrap();
    assert_eq!(report.status, "package_complete");
    assert_eq!(report.worker_manifest_digest, manifest.digest().unwrap());
    assert_eq!(
        report.worker_manifest_file,
        BlobIdentity::of(&manifest_bytes)
    );
    assert_eq!(
        report.payload_digest,
        inventory_digest(&manifest.payload_files()).unwrap()
    );
    assert_eq!(report.payload_file_count, 3);
    assert_eq!(
        report.payload_bytes,
        manifest
            .payload_files()
            .iter()
            .map(|f| f.identity.bytes)
            .sum::<u64>()
    );
    assert_eq!(report.compilation_provenance, "not_evaluated");
    assert_eq!(report.required_runtime_availability, "not_observed");
    assert_eq!(report.native_acceptance, "not_evaluated");
    assert_eq!(report.engineering_acceptance, "not_evaluated");
    assert_eq!(report.job_preparation, "not_evaluated");
    assert!(!report.execution_performed);
    assert_eq!(
        fs::read(f.request.worker_directory.join("worker.json")).unwrap(),
        manifest_bytes
    );
    assert_eq!(fs::read_dir(f.directory.join("var")).unwrap().count(), 1);
}

#[test]
fn verify_and_prepare_share_changed_missing_and_extra_payload_rejection() {
    let f = Fixture::new();
    for name in ["worker/native.exe", "sources/main.rs", "build/recipe.txt"] {
        let path = f.request.worker_directory.join(name);
        let original = fs::read(&path).unwrap();
        for replacement in [Some(b"changed".as_slice()), None] {
            match replacement {
                Some(bytes) => fs::write(&path, bytes).unwrap(),
                None => fs::remove_file(&path).unwrap(),
            }
            assert!(verify_worker(&f.root, &f.request.worker_directory).is_err());
            assert!(prepare(&f.root, &f.request).is_err());
            fs::write(&path, &original).unwrap();
        }
    }
    fs::write(f.request.worker_directory.join("extra.dat"), b"undeclared").unwrap();
    assert!(verify_worker(&f.root, &f.request.worker_directory).is_err());
    assert!(prepare(&f.root, &f.request).is_err());
}

#[test]
fn worker_verification_preserves_strict_manifest_path_and_size_bounds() {
    let f = Fixture::new();
    let path = f.request.worker_directory.join("worker.json");
    let original = fs::read(&path).unwrap();
    let manifest: serde_json::Value = serde_json::from_slice(&original).unwrap();
    for mutation in ["unknown_field", "traversal", "payload_bound", "case_alias"] {
        let mut changed = manifest.clone();
        match mutation {
            "unknown_field" => changed["verified"] = serde_json::json!(true),
            "traversal" => {
                changed["source_files"][0]["path"] = serde_json::json!("sources/../outside.rs")
            }
            "payload_bound" => {
                changed["executable"]["identity"]["bytes"] =
                    serde_json::json!(MAX_PAYLOAD_BYTES + 1)
            }
            "case_alias" => {
                let mut alias = changed["source_files"][0].clone();
                alias["path"] = serde_json::json!("sources/MAIN.RS");
                changed["source_files"].as_array_mut().unwrap().push(alias);
            }
            _ => unreachable!(),
        }
        fs::write(&path, serde_json::to_vec(&changed).unwrap()).unwrap();
        assert!(
            verify_worker(&f.root, &f.request.worker_directory).is_err(),
            "{mutation}"
        );
        assert!(prepare(&f.root, &f.request).is_err(), "{mutation}");
    }
    for bytes in [
        br#"{"schema":"first","schema":"duplicate"}"#.to_vec(),
        vec![b' '; MAX_RECORD_BYTES + 1],
    ] {
        fs::write(&path, bytes).unwrap();
        assert!(verify_worker(&f.root, &f.request.worker_directory).is_err());
        assert!(prepare(&f.root, &f.request).is_err());
    }
}

#[cfg(unix)]
#[test]
fn worker_verification_rejects_outside_root_and_symlinked_members() {
    let f = Fixture::new();
    assert!(verify_worker(&f.root, &PathBuf::from("/etc")).is_err());
    let path = f.request.worker_directory.join("sources/main.rs");
    fs::remove_file(&path).unwrap();
    std::os::unix::fs::symlink(&f.request.inputs[0].source, &path).unwrap();
    assert!(verify_worker(&f.root, &f.request.worker_directory).is_err());
    assert!(prepare(&f.root, &f.request).is_err());
}

#[test]
fn v3_dispatch_streams_large_input_while_archive_stage_stays_closed() {
    let f = Fixture::new();
    let package = prepare(&f.root, &f.request).unwrap();
    let dir = publish(&package, &f.root, &f.directory.join("var/dispatch-test"))
        .unwrap()
        .join("job");
    assert_eq!(dispatch_job(&f.root, &dir).unwrap(), package.job);
    let mut job = package.job.clone();
    job.schema = LARGE_JOB_SCHEMA.into();
    job.capacity=Some(serde_json::from_value(serde_json::json!({"max_input_bytes":268435456u64,"max_input_file_bytes":134217728u64,"max_output_bytes":268435456u64,"max_output_file_bytes":134217728u64,"max_input_files":10000,"max_output_files":10000,"max_segment_bytes":67108864u64})).unwrap());
    let path = dir.join("inputs/large.bin");
    let stream = fs::File::create(&path).unwrap();
    stream.set_len(MAX_PAYLOAD_BYTES + 17).unwrap();
    stream.sync_all().unwrap();
    drop(stream);
    let fingerprint = oe_core::fingerprint_file(&path, 134217728).unwrap();
    job.inputs.push(FileEntry {
        path: RelativePath::new("inputs/large.bin").unwrap(),
        identity: BlobIdentity {
            bytes: fingerprint.bytes,
            sha256: Digest::new(fingerprint.sha256).unwrap(),
        },
    });
    job.validate().unwrap();
    fs::write(dir.join("job.json"), serde_json::to_vec(&job).unwrap()).unwrap();
    assert!(job.require_archive_stage().is_err());
    assert!(from_directory(&f.root, &dir).is_err());
    assert_eq!(dispatch_job(&f.root, &dir).unwrap(), job);
    let mut stream = fs::OpenOptions::new().write(true).open(&path).unwrap();
    stream.write_all(b"x").unwrap();
    stream.sync_all().unwrap();
    drop(stream);
    assert!(dispatch_job(&f.root, &dir).is_err());
    let mut stream = fs::OpenOptions::new().write(true).open(&path).unwrap();
    stream.write_all(&[0]).unwrap();
    stream.sync_all().unwrap();
    drop(stream);
    assert_eq!(dispatch_job(&f.root, &dir).unwrap(), job);
    fs::write(dir.join("unlisted"), b"extra").unwrap();
    assert!(dispatch_job(&f.root, &dir).is_err());
}
