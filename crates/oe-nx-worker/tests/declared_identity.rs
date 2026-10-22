//! Offline declaration checks. Synthetic bytes are never a compiled PE or NX evidence.
use oe_nx_protocol::{Capability, FileEntry, RuntimeIdentity, WORKER_SCHEMA, WorkerManifest};
use oe_nx_worker::identity::{
    NATIVE_WORKER_EXECUTABLE, NATIVE_WORKER_ID, validate_declared_worker_identity,
    validate_worker_identity,
};

fn worker(target: &str) -> WorkerManifest {
    WorkerManifest {
        schema: WORKER_SCHEMA.into(),
        worker_id: NATIVE_WORKER_ID.into(),
        version: "synthetic-role-test".into(),
        target: target.into(),
        executable: FileEntry::from_bytes(NATIVE_WORKER_EXECUTABLE, b"synthetic-not-executed")
            .unwrap(),
        runtime_files: vec![],
        source_files: vec![FileEntry::from_bytes("sources/fixture.rs", b"synthetic").unwrap()],
        build_recipe: FileEntry::from_bytes("build/fixture.txt", b"not compiled").unwrap(),
        toolchain: "synthetic-no-compiler".into(),
        required_runtime: RuntimeIdentity {
            product: "NX".into(),
            product_label: "synthetic-no-sdk".into(),
            architecture: "x64".into(),
            libraries: vec![FileEntry::from_bytes("NXBIN/libufun.dll", b"not loaded").unwrap()],
        },
        capabilities: vec![Capability {
            action: "create_cylinder_solid".into(),
            params_schema: "oev2.nx-rotating-part-params/1".into(),
        }],
    }
}

#[test]
fn supported_declarations_match_without_certifying_host_or_pe() {
    for target in ["x86_64-pc-windows-msvc", "x86_64-pc-windows-gnu"] {
        let declaration = worker(target);
        declaration.validate().unwrap();
        validate_declared_worker_identity(&declaration).unwrap();
        #[cfg(not(windows))]
        assert!(
            format!("{:#}", validate_worker_identity(&declaration).unwrap_err())
                .contains("native_worker_requires_supported_windows_target")
        );
    }
}

#[test]
fn original_cylinder_and_other_wrong_roles_are_protocol_valid_but_refused() {
    for role in [
        "oe-nx-rotating-part",
        "oe-nx-native-assembly",
        "another-role",
    ] {
        let mut declaration = worker("x86_64-pc-windows-msvc");
        declaration.worker_id = role.into();
        declaration.validate().unwrap();
        assert!(
            format!(
                "{:#}",
                validate_declared_worker_identity(&declaration).unwrap_err()
            )
            .contains("native_worker_identity_contract_differs")
        );
    }
}

#[test]
fn changed_path_and_supported_protocol_but_unsupported_native_target_refuse() {
    let mut path = worker("x86_64-pc-windows-msvc");
    path.executable = FileEntry::from_bytes("worker/renamed.exe", b"same synthetic bytes").unwrap();
    path.validate().unwrap();
    assert!(
        format!(
            "{:#}",
            validate_declared_worker_identity(&path).unwrap_err()
        )
        .contains("native_worker_identity_contract_differs")
    );
    let target = worker("aarch64-pc-windows-msvc");
    target.validate().unwrap();
    assert!(
        format!(
            "{:#}",
            validate_declared_worker_identity(&target).unwrap_err()
        )
        .contains("native_worker_requires_supported_windows_target")
    );
}

#[test]
fn old_protocol_inventory_and_schema_guards_remain_mandatory() {
    let mut declaration = worker("x86_64-pc-windows-msvc");
    declaration.schema = "cad-agent.nx-native-worker/v1".into();
    assert!(validate_declared_worker_identity(&declaration).is_err());
    declaration = worker("x86_64-pc-windows-msvc");
    declaration.source_files.clear();
    assert!(validate_declared_worker_identity(&declaration).is_err());
}
