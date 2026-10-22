//! Pure synthetic records; no compiler, shell, SDK, process or NX execution.
use oe_nx_protocol::*;
use oe_windows_controller::build::*;
use std::collections::BTreeMap;

fn installed(path: &str) -> InstalledFile {
    InstalledFile {
        path: path.into(),
        identity: BlobIdentity::of(path.as_bytes()),
    }
}
fn fixture() -> (BuildSpec, BuildEvidence, BTreeMap<String, Vec<u8>>) {
    let mut files = BTreeMap::new();
    // Opaque synthetic bytes intentionally do not claim to be an actual
    // reconstructable archive, lockfile or verified source-tree observation.
    for (name, bytes) in [
        (
            "sources/workspace.zip",
            b"synthetic archive identity; not real build provenance".as_slice(),
        ),
        (
            "sources/workspace-files.json",
            b"synthetic tree record; not an observed extraction".as_slice(),
        ),
        ("sources/Cargo.lock", b"synthetic lock identity".as_slice()),
        (
            "worker/oe-nx-worker.exe",
            b"MZ synthetic; never executed".as_slice(),
        ),
        (
            "worker/nx_shim.dll",
            b"MZ synthetic shim; never loaded".as_slice(),
        ),
        (
            "sources/build/cargo.stdout",
            b"synthetic caller claim".as_slice(),
        ),
        ("sources/build/cargo.stderr", b"".as_slice()),
        (
            "sources/build/shim.stdout",
            b"synthetic caller claim".as_slice(),
        ),
        ("sources/build/shim.stderr", b"".as_slice()),
    ] {
        files.insert(name.into(), bytes.to_vec());
    }
    let entry = |name: &str| FileEntry::from_bytes(name, &files[name]).unwrap();
    let spec = BuildSpec {
        schema: BUILD_SPEC_SCHEMA.into(),
        build_id: JobId::new("fixture-build").unwrap(),
        root_directory: "C:/接收 space/worker-builds".into(),
        root_identity: RootIdentity {
            host_id: "synthetic-host".into(),
            volume_id: "synthetic-volume".into(),
            directory_id: "synthetic-directory".into(),
        },
        source_archive: entry("sources/workspace.zip"),
        source_inventory: entry("sources/workspace-files.json"),
        cargo_lock: entry("sources/Cargo.lock"),
        expanded_source_files: 11_077,
        expanded_source_bytes: 220_150_236,
        tools: BuildTools {
            cargo: installed("C:/Rust tools/bin/cargo.exe"),
            rustc: installed("C:/Rust tools/bin/rustc.exe"),
            gnu_linker: installed("C:/GNU/bin/gcc.exe"),
            msvc_cl: installed("C:/MSVC/bin/cl.exe"),
            msvc_link: installed("C:/MSVC/bin/link.exe"),
            windows_directory: "C:/Windows".into(),
            include_directories: vec!["C:/MSVC/include".into(), "C:/Windows Kits/Include".into()],
            library_directories: vec!["C:/MSVC/lib/x64".into(), "C:/Windows Kits/Lib/x64".into()],
        },
        sdk: InstalledSdk {
            root: "C:/Program Files/Siemens/NX 2412".into(),
            files: [
                "UGOPEN/uf.h",
                "UGOPEN/uf_defs.h",
                "UGOPEN/uf_part.h",
                "UGOPEN/uf_modl_types.h",
                "UGOPEN/uf_object_types.h",
                "UGOPEN/NXOpen/Session.hxx",
                "UGOPEN/libufun.lib",
                "UGOPEN/libnxopencpp.lib",
                "UGOPEN/libnxopencpp_assemblies.lib",
            ]
            .into_iter()
            .map(|name| FileEntry::from_bytes(name, b"declared-only-SDK-identity").unwrap())
            .collect(),
            runtime: RuntimeIdentity {
                product: "Siemens NX".into(),
                product_label: "synthetic-no-installation".into(),
                architecture: "x86_64".into(),
                libraries: vec![
                    FileEntry::from_bytes("NXBIN/libufun.dll", b"declared-only-runtime").unwrap(),
                    FileEntry::from_bytes("NXBIN/libnxopencpp.dll", b"declared-only-cpp-runtime")
                        .unwrap(),
                    FileEntry::from_bytes(
                        "NXBIN/libnxopencpp_assemblies.dll",
                        b"declared-only-assemblies-runtime",
                    )
                    .unwrap(),
                ],
            },
        },
        profile: BuildProfile::Debug,
        jobs: 4,
        max_seconds: 300,
        worker_id: "synthetic-builder-worker".into(),
        worker_version: "fixture".into(),
        declared_capabilities: vec![Capability {
            action: "inspect".into(),
            params_schema: "cad-agent.nx-native-acceptance-params/v2".into(),
        }],
    };
    let plan = spec.plan().unwrap();
    let steps = plan
        .commands
        .iter()
        .enumerate()
        .map(|(i, command)| {
            let prefix = if i == 0 { "cargo" } else { "shim" };
            ObservedBuildStep {
                kind: command.kind,
                command_digest: record_digest(command).unwrap(),
                process: ProcessIdentity {
                    host_id: spec.root_identity.host_id.clone(),
                    pid: 100 + i as u32,
                    creation_time_100ns: 1000 + i as u64,
                    image: command.executable.identity.clone(),
                },
                exit_code: Some(0),
                stdout: entry(&format!("sources/build/{prefix}.stdout")),
                stderr: entry(&format!("sources/build/{prefix}.stderr")),
                output: entry(if i == 0 {
                    "worker/oe-nx-worker.exe"
                } else {
                    "worker/nx_shim.dll"
                }),
            }
        })
        .collect();
    let evidence = BuildEvidence {
        schema: BUILD_EVIDENCE_SCHEMA.into(),
        build_id: spec.build_id.clone(),
        plan_digest: plan.digest().unwrap(),
        root_identity: spec.root_identity.clone(),
        source_inventory_before: spec.source_inventory.identity.clone(),
        source_inventory_after: spec.source_inventory.identity.clone(),
        steps,
    };
    (spec, evidence, files)
}

#[test]
fn typed_plan_preserves_unicode_spaces_and_never_adds_a_shell_or_native_run() {
    let (spec, _, _) = fixture();
    let plan = spec.plan().unwrap();
    assert_eq!(plan.commands.len(), 2);
    assert_eq!(plan.commands[0].arguments[0], "build");
    assert!(plan.commands[0].arguments.contains(&"--locked".into()));
    assert!(plan.commands[0].arguments.contains(&"--offline".into()));
    assert!(
        plan.commands[1]
            .arguments
            .iter()
            .any(|arg| arg.ends_with("/crates/oe-nx-worker/shim/nx_shim.cpp")
                && arg.contains("接收 space"))
    );
    assert!(
        plan.commands[1].arguments.contains(
            &"C:/Program Files/Siemens/NX 2412/UGOPEN/libnxopencpp_assemblies.lib".into()
        )
    );
    assert!(plan.commands.iter().all(|command| {
        command.clear_inherited_environment
            && !command
                .arguments
                .iter()
                .any(|arg| arg == "/C" || arg == "-Command")
    }));
    assert!(plan.output_must_be_new);
    assert!(!plan.network_allowed && !plan.native_execution_requested);
    assert!(plan.new_output_directory.ends_with("/build-fixture-build"));
    assert_eq!(plan.commands[0].environment["RUSTC"], spec.tools.rustc.path);
    assert_eq!(
        plan.commands[0].environment["CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER"],
        spec.tools.gnu_linker.path
    );
    assert!(!plan.commands[0].environment.contains_key("RUSTFLAGS"));
    let mut release = spec;
    release.profile = BuildProfile::Release;
    let release = release.plan().unwrap();
    assert!(release.commands[0].arguments.contains(&"--release".into()));
    assert!(release.commands[0].expected_output.contains("/release/"));
}

#[test]
fn no_arbitrary_shell_environment_flags_or_unsafe_compiler_paths() {
    let (spec, _, _) = fixture();
    for path in [
        "cmd.exe",
        "C:/Windows/cmd.exe",
        "C:/tools/cargo.exe;evil",
        "C:/tools/../cargo.exe",
        "C:/tools/\"cargo.exe",
        "C:/tools/cargo.exe\n",
    ] {
        let mut changed = spec.clone();
        changed.tools.cargo.path = path.into();
        assert!(changed.validate().is_err(), "{path}");
    }
    let mut value = serde_json::to_value(&spec).unwrap();
    value["shell"] = serde_json::json!("cmd /c malicious");
    assert!(decode::<BuildSpec>(&serde_json::to_vec(&value).unwrap()).is_err());
    value.as_object_mut().unwrap().remove("shell");
    value["tools"]["environment"] = serde_json::json!({"RUSTC_WRAPPER":"unreviewed.exe"});
    assert!(decode::<BuildSpec>(&serde_json::to_vec(&value).unwrap()).is_err());
    let mut changed = spec.clone();
    changed.tools.msvc_link.path = "C:/Other/link.exe".into();
    assert!(changed.validate().is_err());
    let mut changed = spec;
    changed.tools.include_directories = vec!["C:/safe;C:/injected".into()];
    assert!(changed.validate().is_err());
}

#[test]
fn expanded_build_budget_is_separate_and_does_not_relax_worker_payload_limits() {
    let (mut spec, _, _) = fixture();
    assert!(
        spec.expanded_source_files > MAX_FILES as u32
            && spec.expanded_source_bytes > MAX_PAYLOAD_BYTES
    );
    assert!(spec.validate().is_ok());
    spec.expanded_source_files = MAX_BUILD_TREE_FILES + 1;
    assert!(spec.validate().is_err());
    spec.expanded_source_files = 11_077;
    spec.expanded_source_bytes = MAX_BUILD_TREE_BYTES + 1;
    assert!(spec.validate().is_err());
    spec.expanded_source_bytes = 220_150_236;
    spec.source_archive.identity.bytes = MAX_PAYLOAD_BYTES + 1;
    assert!(spec.validate().is_err());
}

#[test]
fn externally_filled_exit_zero_never_authenticates_compilation_or_source_truth() {
    let (spec, evidence, files) = fixture();
    let packaged = package_from_external_records(&spec, &evidence, files).unwrap();
    packaged.verify().unwrap();
    assert_eq!(
        packaged.assessment().compilation_execution,
        "not_authenticated_external_records"
    );
    assert_eq!(
        packaged.assessment().source_tree_verification,
        "not_authenticated_external_records"
    );
    assert_eq!(packaged.assessment().native_acceptance, "not_evaluated");
    let raw = &packaged.members()["worker.json"];
    assert_eq!(decode::<WorkerManifest>(raw).unwrap(), *packaged.manifest());
    assert_eq!(packaged.manifest().runtime_files.len(), 1);
    assert!(
        packaged
            .members()
            .keys()
            .all(|name| !name.starts_with("UGOPEN/") && !name.starts_with("NXBIN/"))
    );
    let recipe: serde_json::Value = decode(&packaged.members()["build/recipe.json"]).unwrap();
    assert_eq!(
        recipe["actual_execution"],
        "not_authenticated_external_records"
    );
}

#[test]
fn opaque_external_source_records_are_not_an_archive_verification() {
    let (spec, _, files) = fixture();
    assert!(
        spec.verify_sources(
            &files["sources/workspace.zip"],
            &files["sources/workspace-files.json"],
            &files["sources/Cargo.lock"],
        )
        .is_err()
    );
}

#[test]
fn actual_verified_source_archive_must_match_build_identities_and_counts() {
    use oe_windows_controller::build_source::{SOURCE_SCHEMA, SourceManifest, WORKSPACE_MEMBERS};
    use std::io::{Cursor, Write};
    use zip::{ZipWriter, write::SimpleFileOptions};
    let (mut spec, _, _) = fixture();
    let mut files = BTreeMap::new();
    files.insert(
        "Cargo.toml".to_owned(),
        format!(
            "[workspace]\nresolver=\"3\"\nmembers=[{}]\n[workspace.package]\nversion=\"0.0.1\"\n",
            WORKSPACE_MEMBERS
                .iter()
                .map(|name| format!("\"crates/{name}\""))
                .collect::<Vec<_>>()
                .join(",")
        )
        .into_bytes(),
    );
    let mut lock = "version=4\n".to_owned();
    for name in WORKSPACE_MEMBERS {
        files.insert(
            format!("crates/{name}/Cargo.toml"),
            format!("[package]\nname=\"{name}\"\nversion.workspace=true\n").into_bytes(),
        );
        lock.push_str(&format!(
            "[[package]]\nname=\"{name}\"\nversion=\"0.0.1\"\n"
        ));
    }
    files.insert("Cargo.lock".into(), lock.as_bytes().to_vec());
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in &files {
        zip.start_file(format!("source/{name}"), SimpleFileOptions::default())
            .unwrap();
        zip.write_all(bytes).unwrap();
    }
    let archive = zip.finish().unwrap().into_inner();
    let inventory = serde_json::to_vec(&SourceManifest {
        schema: SOURCE_SCHEMA.into(),
        archive_root: RelativePath::new("source").unwrap(),
        files: files
            .iter()
            .map(|(n, b)| FileEntry::from_bytes(n, b).unwrap())
            .collect(),
    })
    .unwrap();
    spec.source_archive = FileEntry::from_bytes("sources/workspace.zip", &archive).unwrap();
    spec.source_inventory =
        FileEntry::from_bytes("sources/workspace-files.json", &inventory).unwrap();
    spec.cargo_lock = FileEntry::from_bytes("sources/Cargo.lock", lock.as_bytes()).unwrap();
    spec.expanded_source_files = files.len() as u32;
    spec.expanded_source_bytes = files.values().map(|b| b.len() as u64).sum();
    assert_eq!(
        spec.verify_sources(&archive, &inventory, lock.as_bytes())
            .unwrap()
            .members(),
        &files
    );
    for mutation in ["archive", "manifest", "lock", "count", "bytes"] {
        let mut changed = spec.clone();
        match mutation {
            "archive" => changed.source_archive.identity.sha256 = Digest::of(b"different archive"),
            "manifest" => {
                changed.source_inventory.identity.sha256 = Digest::of(b"different manifest")
            }
            "lock" => changed.cargo_lock.identity.sha256 = Digest::of(b"different lock"),
            "count" => changed.expanded_source_files += 1,
            "bytes" => changed.expanded_source_bytes += 1,
            _ => unreachable!(),
        }
        assert!(
            changed
                .verify_sources(&archive, &inventory, lock.as_bytes())
                .is_err(),
            "{mutation}"
        );
    }
}

#[test]
fn missing_failed_unknown_reordered_and_changed_command_observations_cannot_package() {
    let (spec, original, files) = fixture();
    for mutation in [
        "missing",
        "failed",
        "unknown",
        "reordered",
        "command",
        "host",
        "image",
        "reused_process",
        "source_after",
        "root",
        "plan",
        "log_bound",
        "artifact_path",
    ] {
        let mut evidence = original.clone();
        match mutation {
            "missing" => {
                evidence.steps.pop();
            }
            "failed" => evidence.steps[0].exit_code = Some(1),
            "unknown" => evidence.steps[0].exit_code = None,
            "reordered" => evidence.steps.swap(0, 1),
            "command" => evidence.steps[0].command_digest = Digest::of(b"different command"),
            "host" => evidence.steps[0].process.host_id = "different-host".into(),
            "image" => evidence.steps[0].process.image = BlobIdentity::of(b"other compiler"),
            "reused_process" => {
                evidence.steps[1].process.pid = evidence.steps[0].process.pid;
                evidence.steps[1].process.creation_time_100ns =
                    evidence.steps[0].process.creation_time_100ns;
            }
            "source_after" => evidence.source_inventory_after = BlobIdentity::of(b"changed"),
            "root" => evidence.root_identity.directory_id = "changed".into(),
            "plan" => evidence.plan_digest = Digest::of(b"other plan"),
            "log_bound" => evidence.steps[0].stdout.identity.bytes = MAX_RECORD_BYTES as u64 + 1,
            "artifact_path" => {
                evidence.steps[0].output.path =
                    RelativePath::new("worker/not-the-built-output.exe").unwrap()
            }
            _ => unreachable!(),
        }
        assert!(
            package_from_external_records(&spec, &evidence, files.clone()).is_err(),
            "{mutation}"
        );
    }
}

#[test]
fn exact_external_byte_closure_rejects_missing_tampered_and_vendor_payloads() {
    let (spec, evidence, original) = fixture();
    for name in original.keys() {
        let mut files = original.clone();
        files.remove(name);
        assert!(package_from_external_records(&spec, &evidence, files).is_err());
        let mut files = original.clone();
        files.get_mut(name).unwrap().push(1);
        assert!(package_from_external_records(&spec, &evidence, files).is_err());
    }
    let mut files = original;
    files.insert(
        "NXBIN/libufun.dll".into(),
        b"must not distribute vendor DLL".to_vec(),
    );
    assert!(package_from_external_records(&spec, &evidence, files).is_err());
}

#[test]
fn missing_sdk_and_invalid_capabilities_fail_existing_protocol_validation() {
    let (spec, _, _) = fixture();
    let mut changed = spec.clone();
    changed.sdk.files.pop();
    assert!(changed.validate().is_err());
    let mut changed = spec.clone();
    changed.sdk.runtime.libraries.pop();
    assert!(changed.validate().is_err());
    let mut changed = spec.clone();
    changed.tools.rustc.identity.bytes = MAX_PAYLOAD_BYTES + 1;
    assert!(changed.validate().is_err());
    let mut changed = spec.clone();
    changed.declared_capabilities.clear();
    assert!(changed.validate().is_err());
    let mut changed = spec.clone();
    changed.worker_id = "bad\nlabel".into();
    assert!(changed.validate().is_err());
    let mut changed = spec.clone();
    changed.declared_capabilities[0].action = "arbitrary_modeling".into();
    assert!(changed.validate().is_err());
    let mut changed = spec.clone();
    changed.declared_capabilities[0].params_schema = "unimplemented/v1".into();
    assert!(changed.validate().is_err());
    let mut changed = spec;
    changed.jobs = 9;
    assert!(changed.validate().is_err());
}

#[test]
fn recorded_two_library_runtime_is_parseable_but_rejected_by_current_build_contract() {
    // Verbatim field projection from the actual package05 WorkerManifest bytes,
    // SHA256 4bb14d6e0409384c1b86db4901319000d09102122968c109b3fd1cec9e217a9f.
    // The fixture carries no SDK bytes and grants no native acceptance.
    let recorded: serde_json::Value = decode(br#"{
  "required_runtime": {
    "product": "Siemens NX",
    "product_label": "NX 2412 installation; explicitly pinned files have distinct maintenance versions",
    "architecture": "x86_64",
    "libraries": [
      {
        "path": "NXBIN/libnxopencpp.dll",
        "identity": {
          "sha256": "b6f43e51da77b0e21c0b3827966aa8ee3c68b87efbcecfeeb1e9d597223a449a",
          "bytes": 4734992
        }
      },
      {
        "path": "NXBIN/libufun.dll",
        "identity": {
          "sha256": "1279636ac8c489f1b9072722597ae1b0856364705ef8528be5faffd019e1d325",
          "bytes": 1478320
        }
      }
    ]
  },
  "capabilities": [
    {
      "action": "create",
      "params_schema": "cad-agent.nx-native-acceptance-params/v2"
    },
    {
      "action": "inspect",
      "params_schema": "cad-agent.nx-native-acceptance-params/v2"
    },
    {
      "action": "import_step",
      "params_schema": "cad-agent.nx-native-acceptance-params/v2"
    }
  ]
}"#).unwrap();
    let historical_runtime: RuntimeIdentity =
        serde_json::from_value(recorded["required_runtime"].clone()).unwrap();
    historical_runtime.validate().unwrap();
    let (mut spec, _, _) = fixture();
    spec.declared_capabilities = serde_json::from_value(recorded["capabilities"].clone()).unwrap();
    spec.validate().unwrap();
    assert_eq!(spec.plan().unwrap().commands.len(), 2);
    let mut historical_input = spec.clone();
    historical_input.sdk.runtime = historical_runtime;
    assert!(historical_input.validate().is_err());
    let mut wrong = spec.clone();
    wrong.sdk.runtime.product = "NX".into();
    assert!(wrong.validate().is_err());
    let mut wrong = spec;
    wrong.sdk.runtime.architecture = "x64".into();
    assert!(wrong.validate().is_err());
}

#[test]
fn controlled_build_plan_digest_is_stable() {
    let (spec, _, _) = fixture();
    assert_eq!(
        serde_json::to_string(&spec.plan().unwrap().digest().unwrap()).unwrap(),
        "\"98eaefe430f23b3e91eb5eb3dfd59f40132d8e61ecec8469d408b1d00d23c2ce\""
    );
}
