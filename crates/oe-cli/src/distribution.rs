//! Portable transport and explicitly selected local release installation.
//! Local activation does not grant engineering or method-governance approval.
use anyhow::Result;
use clap::Subcommand;
use oe_core::SkillRoot;
use oe_dist::package::{package_skill, verify_archive, verify_package};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// Compare receiver destinations against the same resolved path representation
/// used by SkillRoot. On Windows HOME and USERPROFILE may be distinct; neither
/// is permission to overwrite an existing v1 installation in the other home.
pub(crate) fn require_outside_v1(root: &SkillRoot, target: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        let homes = ["HOME", "USERPROFILE"]
            .into_iter()
            .filter_map(std::env::var_os)
            .map(PathBuf::from)
            .collect::<Vec<_>>();
        require_outside_windows_v1(root, target, &homes)
    }
    #[cfg(not(windows))]
    {
        let _ = root;
        if let Some(home) = std::env::var_os("HOME") {
            anyhow::ensure!(
                !target.starts_with(PathBuf::from(home).join(".codex/skills/ontology-engineering")),
                "receiver_v1_read_only"
            );
        }
        Ok(())
    }
}

#[cfg(windows)]
fn require_outside_windows_v1(root: &SkillRoot, target: &Path, homes: &[PathBuf]) -> Result<()> {
    anyhow::ensure!(!homes.is_empty(), "receiver_home_identity_unavailable");
    let target = root.expand(target)?;
    for home in homes {
        anyhow::ensure!(home.is_absolute(), "receiver_home_identity_unavailable");
        let protected = root.expand(&home.join(".codex/skills/ontology-engineering"))?;
        // Existing components already have their physical spelling. Missing
        // fixed ASCII tail names must also preserve the Windows boundary: an
        // absent ONTOLOGY-ENGINEERING is not a new authorized destination.
        let mut target_components = target.components();
        let protected_prefix = protected.components().all(|expected| {
            target_components.next().is_some_and(|actual| {
                actual
                    .as_os_str()
                    .as_encoded_bytes()
                    .eq_ignore_ascii_case(expected.as_os_str().as_encoded_bytes())
            })
        });
        anyhow::ensure!(!protected_prefix, "receiver_v1_read_only");
    }
    Ok(())
}

#[derive(Subcommand)]
pub enum DistributionCommand {
    /// Assemble, verify or install a bounded Rust release and select it locally.
    Release {
        #[command(subcommand)]
        command: ReleaseCommand,
    },
    /// Build a new private delivery from portable-skill/v1 source identities.
    Package {
        #[arg(long)]
        inventory: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Check all received member bytes against an external inventory and digest.
    Verify {
        #[arg(long)]
        archive: PathBuf,
        #[arg(long)]
        inventory: PathBuf,
        #[arg(long)]
        expected_sha256: String,
    },
}

#[derive(Subcommand)]
pub enum ReleaseCommand {
    /// Check this actual program's versioned semantic build profile. Does not
    /// qualify behavior or change installation/selection.
    CheckProfile {
        #[arg(long)]
        profile: String,
    },
    Assemble {
        #[arg(long)]
        inventory: PathBuf,
        #[arg(long)]
        program_dir: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    Verify {
        #[arg(long)]
        directory: PathBuf,
        #[arg(long)]
        expected_sha256: String,
    },
    Install {
        #[arg(long)]
        source: PathBuf,
        #[arg(long)]
        target: PathBuf,
        #[arg(long)]
        expected_sha256: String,
    },
    InitStore {
        #[arg(long)]
        store: PathBuf,
    },
    Activate {
        #[arg(long)]
        store: PathBuf,
        #[arg(long)]
        directory: PathBuf,
        #[arg(long)]
        expected_sha256: String,
        #[arg(long)]
        expected_current_sha256: Option<String>,
    },
    Rollback {
        #[arg(long)]
        store: PathBuf,
        #[arg(long)]
        expected_current_sha256: String,
    },
    Status {
        #[arg(long)]
        store: PathBuf,
    },
}

fn release(root: &SkillRoot, command: ReleaseCommand) -> Result<Value> {
    use oe_dist::release;
    match command {
        ReleaseCommand::CheckProfile { profile } => {
            let binding = crate::v2::require_execution(root, "release_semantic_profile")?;
            let version = oe_core::read_regular_bytes(&root.path().join("VERSION"), 1024)?;
            let version = std::str::from_utf8(&version)?.trim();
            anyhow::ensure!(
                version == env!("CARGO_PKG_VERSION")
                    && version == semantica_runtime::ENGINE_VERSION,
                "release_semantic_runtime_version_mismatch"
            );
            let observed = oe_dist::release_profile::check(
                &profile,
                version,
                &semantica_runtime::compiled_cargo_features(),
            )?;
            Ok(serde_json::json!({
                "schema":"oev2.release-semantic-profile-check/1",
                "status":"compiled_profile_matches",
                "profile":observed,
                "execution_binding":binding,
                "behavioral_qualification":"not_evaluated",
                "formal_selection":"unchanged",
                "engineering_adoption":"not_granted"
            }))
        }
        ReleaseCommand::Assemble {
            inventory,
            program_dir,
            output,
        } => {
            crate::v2::require_execution(root, "release_assembly")?;
            Ok(serde_json::to_value(oe_dist::release_build::assemble(
                root,
                &inventory,
                &program_dir,
                &output,
            )?)?)
        }
        ReleaseCommand::Verify {
            directory,
            expected_sha256,
        } => Ok(serde_json::to_value(release::verify(
            &root.expand(&directory)?,
            &expected_sha256,
        )?)?),
        ReleaseCommand::Install {
            source,
            target,
            expected_sha256,
        } => {
            let target = root.expand(&target)?;
            anyhow::ensure!(
                !target.starts_with(root.path()) || target.starts_with(root.path().join("var")),
                "receiver_cannot_write_managed_source"
            );
            require_outside_v1(root, &target)?;
            Ok(serde_json::to_value(release::install(
                &root.expand(&source)?,
                &expected_sha256,
                &target,
            )?)?)
        }
        ReleaseCommand::InitStore { store } => {
            release::init_store(&root.private_path(&store)?)?;
            Ok(
                serde_json::json!({"status":"release_store_initialized", "scope":"local_distribution_activation_only"}),
            )
        }
        ReleaseCommand::Activate {
            store,
            directory,
            expected_sha256,
            expected_current_sha256,
        } => Ok(serde_json::to_value(release::activate(
            &root.private_path(&store)?,
            &root.expand(&directory)?,
            &expected_sha256,
            expected_current_sha256.as_deref(),
        )?)?),
        ReleaseCommand::Rollback {
            store,
            expected_current_sha256,
        } => Ok(serde_json::to_value(release::rollback(
            &root.private_path(&store)?,
            &expected_current_sha256,
        )?)?),
        ReleaseCommand::Status { store } => Ok(serde_json::to_value(release::read_active(
            &root.private_path(&store)?,
        )?)?),
    }
}
pub fn run(root: &SkillRoot, command: DistributionCommand) -> Result<Value> {
    match command {
        DistributionCommand::Release { command } => release(root, command),
        DistributionCommand::Package { inventory, output } => {
            let result = package_skill(root, &inventory, &output)?;
            verify_package(&result)?;
            Ok(result.to_json())
        }
        DistributionCommand::Verify {
            archive,
            inventory,
            expected_sha256,
        } => verify_archive(
            &root.expand(&archive)?,
            &root.expand(&inventory)?,
            &expected_sha256,
        ),
    }
}

#[cfg(all(test, windows))]
mod windows_v1_guard_tests {
    use super::*;

    #[test]
    fn windows_v1_guard_compares_resolved_identity_for_both_homes() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let managed = temporary.path().join("managed");
        std::fs::create_dir(&managed)?;
        std::fs::write(managed.join("SKILL.md"), "synthetic")?;
        let root = SkillRoot::new(managed)?;
        let homes = [
            temporary.path().join("HomeA"),
            temporary.path().join("HomeB"),
        ];
        for home in &homes {
            let old = home.join(".codex/skills/ontology-engineering");
            std::fs::create_dir_all(&old)?;
            let native = old.canonicalize()?;
            for target in [
                old.join("new"),
                native.join("new"),
                PathBuf::from(old.to_str().unwrap().to_ascii_uppercase()).join("new"),
            ] {
                let error = require_outside_windows_v1(&root, &target, &homes).unwrap_err();
                assert_eq!(error.to_string(), "receiver_v1_read_only");
            }
            require_outside_windows_v1(
                &root,
                &home.join(".codex/skills/ontology-engineering-v2/new"),
                &homes,
            )?;
        }
        require_outside_windows_v1(&root, &temporary.path().join("unrelated"), &homes)?;
        assert!(require_outside_windows_v1(&root, &temporary.path().join("out"), &[]).is_err());
        assert!(
            require_outside_windows_v1(
                &root,
                &temporary.path().join("out"),
                &[PathBuf::from("relative-home")]
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn windows_v1_guard_rejects_case_alias_of_missing_protected_tail() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let managed = temporary.path().join("managed");
        std::fs::create_dir(&managed)?;
        std::fs::write(managed.join("SKILL.md"), "synthetic")?;
        let root = SkillRoot::new(managed)?;
        let home = temporary.path().join("FakeHome");
        let skills = home.join(".codex/skills");
        std::fs::create_dir_all(&skills)?;
        for target in [
            skills.join("ontology-engineering"),
            skills.join("ONTOLOGY-ENGINEERING"),
            skills.join("ONTOLOGY-ENGINEERING/new"),
        ] {
            assert!(!target.exists());
            assert_eq!(
                require_outside_windows_v1(&root, &target, std::slice::from_ref(&home))
                    .unwrap_err()
                    .to_string(),
                "receiver_v1_read_only"
            );
            assert!(!target.exists());
        }
        require_outside_windows_v1(&root, &skills.join("ONTOLOGY-ENGINEERING-V2"), &[home])?;
        Ok(())
    }
}
