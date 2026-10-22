//! Native executable identity guard, separate from transport/geometry acceptance.
use anyhow::{Context, Result, ensure};
use oe_nx_protocol::{RuntimeIdentity, WorkerManifest};
use std::collections::BTreeSet;

/// Explicit dependencies of the current SDK shim, including ComputedSize.
/// Old three-library workers keep their own historical executable identity.
pub const NATIVE_RUNTIME_PATHS: [&str; 4] = [
    "NXBIN/libufun.dll",
    "NXBIN/libnxopencpp.dll",
    "NXBIN/libnxopencpp_assemblies.dll",
    "NXBIN/libnxopencpp_annotations.dll",
];
pub fn validate_native_runtime(runtime: &RuntimeIdentity) -> Result<()> {
    runtime.validate()?;
    let paths = runtime
        .libraries
        .iter()
        .map(|e| e.path.as_str())
        .collect::<BTreeSet<_>>();
    ensure!(
        runtime.architecture == "x86_64"
            && runtime.libraries.len() == NATIVE_RUNTIME_PATHS.len()
            && paths == NATIVE_RUNTIME_PATHS.into_iter().collect(),
        "native_runtime_pin_set_differs"
    );
    Ok(())
}

#[cfg(all(windows, target_arch = "x86_64", target_env = "gnu"))]
pub const COMPILED_TARGET: Option<&str> = Some("x86_64-pc-windows-gnu");
#[cfg(all(windows, target_arch = "x86_64", target_env = "msvc"))]
pub const COMPILED_TARGET: Option<&str> = Some("x86_64-pc-windows-msvc");
#[cfg(not(any(
    all(windows, target_arch = "x86_64", target_env = "gnu"),
    all(windows, target_arch = "x86_64", target_env = "msvc")
)))]
pub const COMPILED_TARGET: Option<&str> = None;

/// Require the current worker role, executable path and target.
pub const NATIVE_WORKER_ID: &str = "oe-nx-native-acceptance";
pub const NATIVE_WORKER_EXECUTABLE: &str = "worker/oe-nx-worker.exe";

fn worker_identity_for_target(worker: &WorkerManifest, expected_target: &str) -> Result<()> {
    ensure!(
        worker.worker_id == NATIVE_WORKER_ID
            && worker.target == expected_target
            && worker.executable.path.as_str() == NATIVE_WORKER_EXECUTABLE,
        "native_worker_identity_contract_differs"
    );
    Ok(())
}

/// Offline declaration check using the same role/path contract as the native
/// executable. It neither executes that PE nor certifies its actual target,
/// build, installed runtime, geometry or engineering suitability.
pub fn validate_declared_worker_identity(worker: &WorkerManifest) -> Result<()> {
    worker.validate()?;
    ensure!(
        ["x86_64-pc-windows-gnu", "x86_64-pc-windows-msvc"].contains(&worker.target.as_str()),
        "native_worker_requires_supported_windows_target"
    );
    worker_identity_for_target(worker, &worker.target)
}

pub fn validate_worker_identity(worker: &WorkerManifest) -> Result<()> {
    let compiled = COMPILED_TARGET.context("native_worker_requires_supported_windows_target")?;
    worker_identity_for_target(worker, compiled)
}
