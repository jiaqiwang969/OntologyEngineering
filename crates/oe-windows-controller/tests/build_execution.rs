//! Portable planning over anonymous source bytes; no disk tree or compiler.
use oe_nx_protocol::*;
use oe_windows_controller::{build::*, build_execution::*};

#[path = "support/build_execution_fixture.rs"]
mod support;
use support::fixture;

#[test]
fn new_owned_layout_and_explicit_environment_are_deterministic() {
    let (spec, source) = fixture();
    let prepared = prepare_plan(&spec, &source).unwrap();
    let record = prepared.record();
    assert_eq!(
        record.source_directory,
        "C:/接收 space/worker-builds/build-fixture-build/source/workspace"
    );
    assert_eq!(record.commands.len(), 2);
    assert_eq!(record.source.archive, spec.build.source_archive.identity);
    assert_eq!(
        record.source.expanded_bytes,
        source.assessment().expanded_bytes
    );
    assert_eq!(
        record.source.root_cargo_manifest,
        source.assessment().root_cargo_manifest
    );
    assert!(record.output_must_be_new);
    for entry in &record.commands {
        let c = &entry.command;
        assert!(c.clear_inherited_environment);
        assert!(
            c.working_directory
                .starts_with(&record.new_output_directory)
        );
        assert!(c.expected_output.starts_with(&record.new_output_directory));
        for name in ["CARGO_HOME", "CARGO_TARGET_DIR", "TEMP", "TMP"] {
            assert!(c.environment[name].starts_with(&format!("{}/", record.new_output_directory)));
        }
        assert_eq!(c.environment["TEMP"], c.environment["TMP"]);
        assert_eq!(
            c.environment["PATH"],
            "C:/MSVC/bin;C:/GNU/bin;C:/Rust tools/bin;C:/Windows/System32"
        );
        assert!(!c.environment.contains_key("RUSTFLAGS"));
        assert!(!c.environment.contains_key("RUSTC_WRAPPER"));
        assert_eq!(entry.windows.command_line_utf16.last(), Some(&0));
        assert_eq!(entry.windows.application_name, c.executable.path);
    }
    assert_eq!(record.commands[0].command.environment.len(), 9);
    assert_eq!(record.commands[1].command.environment.len(), 11);
    assert!(
        !record.commands[0]
            .command
            .environment
            .contains_key("INCLUDE")
    );
    assert_eq!(
        record.commands[1].command.environment["INCLUDE"],
        "C:/MSVC/include;C:/Windows Kits/Include"
    );
    assert_eq!(
        record.commands[0].command.arguments,
        [
            "build",
            "--locked",
            "--offline",
            "--target",
            "x86_64-pc-windows-gnu",
            "-p",
            "oe-nx-worker",
            "--bin",
            "oe-nx-worker",
            "--jobs",
            "4"
        ]
    );
    assert_eq!(
        prepared.digest().unwrap(),
        prepare_plan(&spec, &source).unwrap().digest().unwrap()
    );
    let decoded: ExecutionPlanRecord = decode(&serde_json::to_vec(record).unwrap()).unwrap();
    decoded.validate_against(&spec, &source).unwrap();
}

#[test]
fn cargo_offline_never_claims_os_denial_or_execution_and_v1_is_not_upgraded() {
    let (mut spec, source) = fixture();
    let plan = prepare_plan(&spec, &source).unwrap();
    let assessment = &plan.record().assessment;
    assert_eq!(assessment.os_network_enforcement, "not_provided");
    assert_eq!(assessment.network_activity, "not_observed");
    assert_eq!(assessment.process_execution, "not_started");
    assert_eq!(
        assessment.installed_tools_sdk_and_root,
        "declared_identities_not_observed"
    );
    assert_eq!(assessment.filesystem_materialization, "not_performed");
    assert_eq!(assessment.cargo_resolution, "not_executed");
    assert_eq!(assessment.native_execution, "not_requested");
    spec.network_policy = NetworkPolicy::RequireOsDenied;
    assert!(prepare_plan(&spec, &source).is_err());
    assert!(decode::<ExecutionSpecV2>(&serde_json::to_vec(&spec.build).unwrap()).is_err());
    assert!(
        decode::<ExecutionPlanRecord>(&serde_json::to_vec(&spec.build.plan().unwrap()).unwrap())
            .is_err()
    );
    assert!(
        spec.build
            .plan()
            .unwrap()
            .commands
            .iter()
            .all(|c| !c.environment.contains_key("TEMP"))
    );
}

#[test]
fn source_proof_cannot_be_substituted_by_a_declared_hash_or_count() {
    let (spec, source) = fixture();
    for field in 0..5 {
        let mut changed = spec.clone();
        match field {
            0 => changed.build.source_archive.identity = BlobIdentity::of(b"other"),
            1 => changed.build.source_inventory.identity = BlobIdentity::of(b"other"),
            2 => changed.build.cargo_lock.identity = BlobIdentity::of(b"other"),
            3 => changed.build.expanded_source_files += 1,
            _ => changed.build.expanded_source_bytes += 1,
        }
        changed.validate().unwrap();
        assert!(prepare_plan(&changed, &source).is_err());
    }
}

#[test]
fn decoded_records_with_altered_commands_bindings_or_success_claims_are_rejected() {
    let (spec, source) = fixture();
    let record = prepare_plan(&spec, &source).unwrap().record().clone();
    for i in 0..12 {
        let mut altered = record.clone();
        match i {
            0 => altered.commands[0]
                .command
                .arguments
                .push("--config=evil".into()),
            1 => {
                altered.commands[0]
                    .command
                    .environment
                    .insert("RUSTC_WRAPPER".into(), "unapproved.exe".into());
            }
            2 => altered.commands[1].command.executable.path = "C:/Windows/cmd.exe".into(),
            3 => altered.source_directory = "C:/shared/source".into(),
            4 => altered.source.inventory = BlobIdentity::of(b"forged inventory"),
            5 => altered.assessment.process_execution = "succeeded".into(),
            6 => altered.assessment.os_network_enforcement = "denied".into(),
            7 => altered.root_identity.directory_id = "different directory".into(),
            8 => altered.output_must_be_new = false,
            9 => altered.commands[0].windows.command_line_utf16.push(65),
            10 => altered.source.root_cargo_manifest = BlobIdentity::of(b"other root"),
            _ => altered.network_policy = NetworkPolicy::RequireOsDenied,
        }
        // Valid JSON records still do not authenticate a plan or any execution.
        let decoded: ExecutionPlanRecord = decode(&serde_json::to_vec(&altered).unwrap()).unwrap();
        assert!(
            decoded.validate_against(&spec, &source).is_err(),
            "case {i}"
        );
    }
    for nested in ["", "source", "assessment"] {
        let mut json = serde_json::to_value(&record).unwrap();
        let target = if nested.is_empty() {
            &mut json
        } else {
            &mut json[nested]
        };
        target["executed"] = serde_json::json!(true);
        assert!(decode::<ExecutionPlanRecord>(&serde_json::to_vec(&json).unwrap()).is_err());
    }
    let mut json = serde_json::to_value(&spec).unwrap();
    json["arguments"] = serde_json::json!(["arbitrary"]);
    assert!(decode::<ExecutionSpecV2>(&serde_json::to_vec(&json).unwrap()).is_err());
}

#[test]
fn new_directory_cannot_contain_declared_installed_inputs_even_with_aliases() {
    let (spec, source) = fixture();
    for path in [
        "C:/接收 space/worker-builds/build-fixture-build/tools/cargo.exe",
        r"c:\接收 space\worker-builds\BUILD-FIXTURE-BUILD\tools\cargo.exe",
        "C:/接收 space/worker-builds/build-ｆixture-build/tools/cargo.exe",
    ] {
        let mut changed = spec.clone();
        changed.build.tools.cargo.path = path.into();
        assert!(prepare_plan(&changed, &source).is_err(), "{path}");
    }
    let mut changed = spec.clone();
    changed.build.tools.include_directories =
        vec!["C:/接收 space/worker-builds/build-fixture-build/includes".into()];
    assert!(prepare_plan(&changed, &source).is_err());
    // A sibling sharing a prefix is not the output subtree.
    let mut allowed = spec;
    allowed.build.tools.cargo.path =
        "C:/接收 space/worker-builds/build-fixture-build-tools/cargo.exe".into();
    prepare_plan(&allowed, &source).unwrap();
}

#[test]
fn oversized_paths_are_rejected_and_percent_paths_remain_literal() {
    let (spec, source) = fixture();
    let mut changed = spec.clone();
    changed.build.tools.include_directories = (0..32)
        .map(|i| format!("C:/include{i}/{}", "x".repeat(980)))
        .collect();
    changed.build.tools.library_directories = (0..32)
        .map(|i| format!("C:/library{i}/{}", "x".repeat(980)))
        .collect();
    assert!(changed.validate().is_err());
    assert!(prepare_plan(&changed, &source).is_err());
    let mut literal = spec;
    literal.build.root_directory = "C:/builds/%TEMP%&literal 空间".into();
    literal.build.profile = BuildProfile::Release;
    let prepared = prepare_plan(&literal, &source).unwrap();
    assert!(
        prepared.record().commands[0]
            .command
            .arguments
            .contains(&"--release".into())
    );
    let shim = &prepared.record().commands[1];
    let raw = String::from_utf16(
        &shim.windows.command_line_utf16[..shim.windows.command_line_utf16.len() - 1],
    )
    .unwrap();
    assert!(raw.contains("%TEMP%&literal 空间"));
    assert!(raw.starts_with("\"C:/MSVC/bin/cl.exe\" "));
}
