//! Explicit, local release transport and activation. This does not select a
//! qualified method/runtime in the existing evolution owner, grant engineering
//! acceptance, or change any global executable. Apple Silicon macOS and Windows x86_64 MSVC use distinct manifest
//! contracts. The outer bundle keeps runtime programs outside the source root.
#[cfg(not(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(windows, target_arch = "x86_64", target_env = "msvc")
)))]
use anyhow::Result;
use serde::{Deserialize, Serialize};
#[cfg(not(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(windows, target_arch = "x86_64", target_env = "msvc")
)))]
use std::path::Path;
use std::path::PathBuf;

pub use oe_contracts::{ReleaseFile, ReleaseManifest};

pub const MANIFEST_NAME: &str = "RELEASE-MANIFEST.json";
#[cfg(not(windows))]
pub const TARGET: &str = "aarch64-apple-darwin";
#[cfg(windows)]
pub const TARGET: &str = "x86_64-pc-windows-msvc";
#[cfg(not(windows))]
pub const SCHEMA: &str = "oev2.release-manifest/1";
#[cfg(windows)]
pub const SCHEMA: &str = "oev2.release-manifest/2";
#[cfg(not(windows))]
pub const ENTRY: &str = "bin/oev2";
#[cfg(windows)]
pub const ENTRY: &str = "bin/oev2.exe";
pub const SCOPE: &str = "local_distribution_activation_only";
#[cfg(not(windows))]
pub const PROGRAM_NAMES: &[&str] = &[
    "oev2",
    "oev2-design-bootstrap",
    "oe-code-runner",
    "oe-code-evaluator",
    "oe-evolution-driver",
    "oe-runtime-linker2",
    "oe-runtime-rustc2",
];

#[cfg(windows)]
pub const PROGRAM_NAMES: &[&str] = &[
    "oev2.exe",
    "oev2-design-bootstrap.exe",
    "oe-code-runner.exe",
    "oe-code-evaluator.exe",
    "oe-evolution-driver.exe",
    "oe-runtime-linker2.exe",
    "oe-runtime-rustc2.exe",
];
#[cfg(all(windows, target_arch = "x86_64", target_env = "msvc"))]
#[path = "release_windows_fs.rs"]
pub(crate) mod windows_fs;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProgramReference {
    pub path: PathBuf,
    pub sha256: String,
    pub bytes: u64,
    pub mode: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReleaseReference {
    pub root: PathBuf,
    pub manifest_sha256: String,
    pub version: String,
    pub target: String,
}

/// The wire contract uses the design compiler's u64 scalar; OS permissions stay
/// u32 and are converted only after rejecting every non-permission bit.
pub(crate) fn permission_mode(mode: u64) -> anyhow::Result<u32> {
    anyhow::ensure!(mode <= 0o777, "release_mode_out_of_range");
    Ok(u32::try_from(mode)?)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct VerifiedRelease {
    pub root: PathBuf,
    pub skill_root: PathBuf,
    pub manifest_sha256: String,
    pub version: String,
    pub target: String,
    pub entry: PathBuf,
    pub programs: Vec<ProgramReference>,
}

impl VerifiedRelease {
    pub fn reference(&self) -> ReleaseReference {
        ReleaseReference {
            root: self.root.clone(),
            manifest_sha256: self.manifest_sha256.clone(),
            version: self.version.clone(),
            target: self.target.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ActivationRecord {
    pub schema: String,
    pub generation: u64,
    pub operation: String,
    pub scope: String,
    pub current: ReleaseReference,
    pub previous: Option<ReleaseReference>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ActiveRelease {
    pub record_sha256: String,
    pub record: ActivationRecord,
    pub release: VerifiedRelease,
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[path = "release_macos.rs"]
mod native;
#[cfg(all(windows, target_arch = "x86_64", target_env = "msvc"))]
#[path = "release_windows.rs"]
mod native;
#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(windows, target_arch = "x86_64", target_env = "msvc")
))]
pub use native::{activate, init_store, install, read_active, rollback, verify};

#[cfg(not(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(windows, target_arch = "x86_64", target_env = "msvc")
)))]
fn unsupported<T>() -> Result<T> {
    anyhow::bail!("release_platform_unsupported")
}
#[cfg(not(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(windows, target_arch = "x86_64", target_env = "msvc")
)))]
pub fn verify(_: &Path, _: &str) -> Result<VerifiedRelease> {
    unsupported()
}
#[cfg(not(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(windows, target_arch = "x86_64", target_env = "msvc")
)))]
pub fn install(_: &Path, _: &str, _: &Path) -> Result<VerifiedRelease> {
    unsupported()
}
#[cfg(not(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(windows, target_arch = "x86_64", target_env = "msvc")
)))]
pub fn init_store(_: &Path) -> Result<()> {
    unsupported()
}
#[cfg(not(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(windows, target_arch = "x86_64", target_env = "msvc")
)))]
pub fn activate(_: &Path, _: &Path, _: &str, _: Option<&str>) -> Result<ActiveRelease> {
    unsupported()
}
#[cfg(not(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(windows, target_arch = "x86_64", target_env = "msvc")
)))]
pub fn rollback(_: &Path, _: &str) -> Result<ActiveRelease> {
    unsupported()
}
#[cfg(not(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(windows, target_arch = "x86_64", target_env = "msvc")
)))]
pub fn read_active(_: &Path) -> Result<ActiveRelease> {
    unsupported()
}
