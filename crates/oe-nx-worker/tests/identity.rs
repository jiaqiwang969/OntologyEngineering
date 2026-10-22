//! Checks only the compiled executable identity guard. No NX DLL is loaded.
#![cfg(all(windows, target_arch = "x86_64"))]
use oe_nx_protocol::{Capability, FileEntry, RuntimeIdentity, WORKER_SCHEMA, WorkerManifest};
use oe_nx_worker::identity::validate_worker_identity;

fn worker(target: &str) -> WorkerManifest {
    WorkerManifest {
        schema: WORKER_SCHEMA.into(),
        worker_id: "oe-nx-native-acceptance".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        target: target.into(),
        executable: FileEntry::from_bytes(
            "worker/oe-nx-worker.exe",
            b"synthetic-guard-only-not-executed",
        )
        .unwrap(),
        runtime_files: vec![],
        source_files: vec![
            FileEntry::from_bytes("sources/guard.rs", b"synthetic-guard-only").unwrap(),
        ],
        build_recipe: FileEntry::from_bytes(
            "build/recipe.txt",
            b"synthetic-guard-only-no-native-acceptance",
        )
        .unwrap(),
        toolchain: "synthetic-guard-fixture".into(),
        required_runtime: RuntimeIdentity {
            product: "NX".into(),
            product_label: "synthetic-not-loaded".into(),
            architecture: "x64".into(),
            libraries: vec![
                FileEntry::from_bytes("NXBIN/libufun.dll", b"synthetic-not-loaded").unwrap(),
            ],
        },
        capabilities: vec![Capability {
            action: "inspect".into(),
            params_schema: "cad-agent.nx-native-acceptance-params/v2".into(),
        }],
    }
}
#[test]
fn compiled_windows_target_is_accepted_and_another_target_is_rejected() {
    #[cfg(target_env = "msvc")]
    let (actual, other) = ("x86_64-pc-windows-msvc", "x86_64-pc-windows-gnu");
    #[cfg(target_env = "gnu")]
    let (actual, other) = ("x86_64-pc-windows-gnu", "x86_64-pc-windows-msvc");
    let matched = worker(actual);
    matched.validate().unwrap();
    assert!(
        validate_worker_identity(&matched).is_ok(),
        "compiled target was refused"
    );
    let different = worker(other);
    different.validate().unwrap();
    assert!(
        validate_worker_identity(&different).is_err(),
        "foreign target was accepted"
    );
}
#[test]
fn a_protocol_valid_foreign_target_never_matches_the_compiled_worker() {
    #[cfg(target_env = "msvc")]
    let other = "x86_64-pc-windows-gnu";
    #[cfg(target_env = "gnu")]
    let other = "x86_64-pc-windows-msvc";
    let different = worker(other);
    different.validate().unwrap();
    assert!(
        validate_worker_identity(&different).is_err(),
        "foreign target was accepted"
    );
}
