//! Explicitly retired external Python installer; historical recovery fixtures
//! compile only inside the Rust unit-test module, never the library artifact.
use anyhow::{Result, bail};
use oe_core::SkillRoot;
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct InstallOptions {
    pub python: PathBuf,
    /// New transaction directory below the owning skill's var/.
    pub output: PathBuf,
    pub wheelhouse: Option<PathBuf>,
    pub offline: bool,
}
impl InstallOptions {
    pub fn new(python: PathBuf, output: PathBuf) -> Self {
        Self {
            python,
            output,
            wheelhouse: None,
            offline: false,
        }
    }
}

pub fn preflight(_root: &SkillRoot, _python: &Path, _output: &Path) -> Result<Value> {
    bail!("legacy_python_runtime_install_retired_in_v2")
}

pub fn install(_root: &SkillRoot, _options: &InstallOptions) -> Result<Value> {
    bail!("legacy_python_runtime_install_retired_in_v2")
}

#[cfg(test)]
#[allow(dead_code, unused_imports)]
#[path = "runtime_install_history.rs"]
mod historical_recovery_fixtures;
