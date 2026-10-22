use anyhow::{Result, ensure};
use clap::Subcommand;
use oe_contracts::{EngineeringExplorationPort, EngineeringExplorationRunRequest};
use oe_core::{SkillRoot, read_regular_bytes, sha256};
use serde_json::{Value, json};
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum ExplorationCommand {
    /// Expose the next frozen action or submit its source-bound host result.
    Advance {
        #[arg(long)]
        input: PathBuf,
    },
    /// Freeze complete Project/work, hypotheses, discriminating actions and criteria.
    Freeze {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Recheck the live sources, complete task and frozen original bytes.
    Verify {
        #[arg(long)]
        frozen: PathBuf,
    },
    /// Execute frozen actions, read results, and retain scoped decisions in a new copy.
    Run {
        #[arg(long)]
        frozen: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
}
pub fn run(root: &SkillRoot, command: ExplorationCommand) -> Result<Value> {
    let binding = crate::v2::require_execution(root, "engineering_exploration")?;
    let path = match &command {
        ExplorationCommand::Freeze { input, .. } | ExplorationCommand::Advance { input } => input,
        ExplorationCommand::Verify { frozen } | ExplorationCommand::Run { frozen, .. } => frozen,
    };
    let path = root.expand(path)?;
    ensure!(
        path.starts_with(root.path()),
        "exploration_cli_input_outside_root"
    );
    let raw = read_regular_bytes(&path, 16 * 1024 * 1024)?;
    let reference = oe_contracts::ArtifactReference {
        path: oe_core::portable_source_path(root.path(), &path)?,
        sha256: sha256(&raw),
    };
    let output = match &command {
        ExplorationCommand::Advance { .. } => {
            let request: oe_contracts::HostExplorationInput =
                serde_json::from_value(oe_core::json::strict_json(&raw)?)?;
            Some(root.expand(std::path::Path::new(&request.output))?)
        }
        ExplorationCommand::Freeze { output, .. } | ExplorationCommand::Run { output, .. } => {
            Some(root.expand(output)?)
        }
        _ => None,
    };
    let mut result = match command {
        ExplorationCommand::Advance { .. } => serde_json::to_value(
            oe_engineering::workflow::exploration::host::advance(root, &reference)?,
        )?,
        ExplorationCommand::Freeze { output, .. } => {
            oe_engineering::workflow::exploration::freeze(root, &reference, &output)?
        }
        ExplorationCommand::Verify { .. } => {
            oe_engineering::workflow::exploration::verify(root, &reference)?
        }
        ExplorationCommand::Run { output, .. } => {
            let request = EngineeringExplorationRunRequest {
                frozen: reference.clone(),
                output: oe_core::portable_source_path(root.path(), &root.expand(&output)?)?,
            };
            serde_json::to_value(
                oe_engineering::workflow::exploration::ExplorationExecutor { root }
                    .execute(&request)
                    .map_err(|_| {
                        anyhow::anyhow!("exploration_execution_failed_inspect_partial_records")
                    })?,
            )?
        }
    };
    ensure!(
        read_regular_bytes(&path, 16 * 1024 * 1024)? == raw,
        "exploration_cli_input_changed"
    );
    let recheck = crate::v2::require_execution(root, "engineering_exploration_completion")?;
    for key in [
        "normalizer",
        "cli",
        "semantic_contracts_sha256",
        "executable_sha256",
    ] {
        ensure!(
            binding[key] == recheck[key],
            "exploration_cli_execution_identity_changed"
        );
    }
    if let Some(dir) = output {
        oe_core::write_new_bytes(&dir.join("cli-input.json"), &raw)?;
        oe_core::write_new(&dir.join("execution-binding.json"), &binding)?;
        oe_core::write_new(&dir.join("execution-recheck.json"), &recheck)?;
        oe_core::write_new(
            &dir.join("cli-completed.json"),
            &json!({"schema":"oev2.exploration-cli-completed/1",
            "input":reference,"core_completed_sha256":sha256(read_regular_bytes(&dir.join("completed.json"),16*1024*1024)?),
            "original_project_mutated":false,"adopted":false}),
        )?;
    }
    result["execution_binding"] = binding;
    Ok(result)
}
