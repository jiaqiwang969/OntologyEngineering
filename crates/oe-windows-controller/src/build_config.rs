//! Deliberately finite Cargo configuration; no dependency resolution or shell.
use crate::{build_execution::PreparedExecutionPlan, build_source::VerifiedSourceArchive};
use anyhow::{Context, Result, ensure};
use oe_nx_protocol::{Digest, FileEntry, MAX_RECORD_BYTES};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CargoConfigRecord {
    pub schema: String,
    pub plan_digest: Digest,
    pub workspace_config: Option<FileEntry>,
    pub workspace_config_policy: String,
    pub target_override_environment: String,
    pub generated_config: FileEntry,
    pub vendor_directory: Option<String>,
    pub registry_packages: usize,
    pub vendor_checksums: String,
    pub cargo_resolution: String,
    pub ancestor_configuration: String,
}
/// Constructed from verified source only. Its bytes are not yet on disk.
pub struct PreparedCargoConfig {
    record: CargoConfigRecord,
    bytes: Vec<u8>,
}
impl PreparedCargoConfig {
    pub fn record(&self) -> &CargoConfigRecord {
        &self.record
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

pub fn prepare_config(
    plan: &PreparedExecutionPlan,
    source: &VerifiedSourceArchive,
) -> Result<PreparedCargoConfig> {
    let binding = &plan.record().source;
    let assessment = source.assessment();
    ensure!(
        binding.archive == assessment.archive
            && binding.inventory == assessment.manifest
            && binding.cargo_lock == assessment.cargo_lock,
        "Cargo configuration source proof differs"
    );
    let members = source.members();
    ensure!(
        !members.contains_key(".cargo/config"),
        "legacy workspace Cargo config is unsupported"
    );
    let workspace_config = members
        .get(".cargo/config.toml")
        .map(|bytes| {
            ensure!(
                bytes.len() <= MAX_RECORD_BYTES,
                "Cargo config exceeds bound"
            );
            let config: toml::Table = toml::from_str(std::str::from_utf8(bytes)?)?;
            ensure!(
                config.len() == 1,
                "unsupported workspace Cargo config tables"
            );
            let build = config
                .get("build")
                .and_then(toml::Value::as_table)
                .context("only the fixed build target-dir config is supported")?;
            ensure!(
                build.len() == 1
                    && build.get("target-dir").and_then(toml::Value::as_str)
                        == Some("var/builds/rust-target"),
                "unsupported workspace Cargo build settings"
            );
            FileEntry::from_bytes(".cargo/config.toml", bytes)
        })
        .transpose()?;
    let vendor_directory = if binding.registry_packages > 0 {
        ensure!(
            members.keys().any(|name| name.starts_with("vendor/")),
            "registry source requires an included vendor tree"
        );
        Some(format!("{}/vendor", plan.record().source_directory))
    } else {
        None
    };
    let mut text = String::new();
    if let Some(directory) = &vendor_directory {
        // Paths already exclude controls; JSON string escaping is also valid
        // for these TOML basic strings. It is never used as shell quoting.
        text.push_str(&format!(
            "[source.crates-io]\nreplace-with = \"oe-owned-vendor\"\n\n[source.oe-owned-vendor]\ndirectory = {}\n\n",
            serde_json::to_string(directory)?
        ));
    }
    text.push_str("[net]\noffline = true\n");
    let bytes = text.into_bytes();
    let record = CargoConfigRecord {
        schema: "cad-agent.nx-worker-cargo-config/v1".into(),
        plan_digest: plan.digest()?,
        workspace_config,
        workspace_config_policy: "fixed_target_dir_only_overridden_by_environment".into(),
        target_override_environment: "CARGO_TARGET_DIR".into(),
        generated_config: FileEntry::from_bytes("cargo-home/config.toml", &bytes)?,
        vendor_directory,
        registry_packages: binding.registry_packages,
        vendor_checksums: "not_evaluated".into(),
        cargo_resolution: "not_executed".into(),
        ancestor_configuration: "requires_windows_absence_observation_and_prelaunch_recheck".into(),
    };
    Ok(PreparedCargoConfig { record, bytes })
}
