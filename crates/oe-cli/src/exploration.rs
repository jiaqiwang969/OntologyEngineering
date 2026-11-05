use anyhow::{ensure, Result};
use clap::Subcommand;
use oe_contracts::{EngineeringExplorationPort, EngineeringExplorationRunRequest};
use oe_core::{read_regular_bytes, sha256, SkillRoot};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
pub enum ExplorationCommand {
    /// Derive source-bound engineering relations and explain current next work.
    Reasoning {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Rebuild a prior reasoning view from its current original sources.
    VerifyReasoning {
        #[arg(long)]
        input: PathBuf,
    },
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
fn input_reference(
    root: &SkillRoot,
    input: &Path,
) -> Result<(PathBuf, Vec<u8>, oe_contracts::ArtifactReference)> {
    let path = root.data_expand(input)?;
    ensure!(
        path.starts_with(root.data_path()),
        "exploration_cli_input_outside_root"
    );
    let raw = read_regular_bytes(&path, 16 * 1024 * 1024)?;
    let reference = oe_contracts::ArtifactReference {
        path: root.portable_data_path(&path)?,
        sha256: sha256(&raw),
    };
    Ok((path, raw, reference))
}
fn recheck_execution(root: &SkillRoot, binding: &Value) -> Result<Value> {
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
    Ok(recheck)
}
pub fn run(root: &SkillRoot, command: ExplorationCommand) -> Result<Value> {
    let binding = crate::v2::require_execution(root, "engineering_exploration")?;
    if let ExplorationCommand::Reasoning { input, output } = &command {
        let (path, raw, _) = input_reference(root, input)?;
        let request = serde_json::from_value(oe_core::json::strict_json(&raw)?)?;
        let view = oe_engineering::workflow::exploration::reasoning::derive(root, &request)?;
        ensure!(
            read_regular_bytes(&path, 16 * 1024 * 1024)? == raw,
            "reasoning_cli_input_changed"
        );
        recheck_execution(root, &binding)?;
        oe_core::write_new(&root.private_path(output)?, &serde_json::to_value(&view)?)?;
        return Ok(
            json!({"schema":"oev2.engineering-reasoning-cli/1","status":"derived","view":view,"execution_binding":binding}),
        );
    }
    if let ExplorationCommand::VerifyReasoning { input } = &command {
        let (path, raw, _) = input_reference(root, input)?;
        let view = serde_json::from_value(oe_core::json::strict_json(&raw)?)?;
        oe_engineering::workflow::exploration::reasoning::verify(root, &view)?;
        ensure!(
            read_regular_bytes(&path, 16 * 1024 * 1024)? == raw,
            "reasoning_cli_input_changed"
        );
        recheck_execution(root, &binding)?;
        return Ok(
            json!({"schema":"oev2.engineering-reasoning-verification/1","status":"current_reasoning_verified","execution_binding":binding}),
        );
    }
    let path = match &command {
        ExplorationCommand::Freeze { input, .. } | ExplorationCommand::Advance { input } => input,
        ExplorationCommand::Verify { frozen } | ExplorationCommand::Run { frozen, .. } => frozen,
        ExplorationCommand::Reasoning { .. } | ExplorationCommand::VerifyReasoning { .. } => {
            unreachable!()
        }
    };
    let (path, raw, reference) = input_reference(root, path)?;
    let output = match &command {
        ExplorationCommand::Advance { .. } => {
            let request: oe_contracts::HostExplorationInput =
                serde_json::from_value(oe_core::json::strict_json(&raw)?)?;
            Some(root.private_path(Path::new(&request.output))?)
        }
        ExplorationCommand::Freeze { output, .. } | ExplorationCommand::Run { output, .. } => {
            Some(root.private_path(output)?)
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
                output: root.portable_data_path(&root.private_path(&output)?)?,
            };
            serde_json::to_value(
                oe_engineering::workflow::exploration::ExplorationExecutor { root }
                    .execute(&request)
                    .map_err(|_| {
                        anyhow::anyhow!("exploration_execution_failed_inspect_partial_records")
                    })?,
            )?
        }
        ExplorationCommand::Reasoning { .. } | ExplorationCommand::VerifyReasoning { .. } => {
            unreachable!()
        }
    };
    ensure!(
        read_regular_bytes(&path, 16 * 1024 * 1024)? == raw,
        "exploration_cli_input_changed"
    );
    let recheck = recheck_execution(root, &binding)?;
    if let Some(dir) = output {
        if result["schema"] == "oev2.exploration-host-state/1" {
            let (_, _, reference) = input_reference(root, &dir.join("completed.json"))?;
            let (_, _, view) = oe_engineering::workflow::exploration::reasoning::inspect_recorded(
                root, &reference,
            )?;
            oe_core::write_new(&dir.join("reasoning.json"), &serde_json::to_value(&view)?)?;
            let (_, _, reference) = input_reference(root, &dir.join("reasoning.json"))?;
            result["reasoning"] = json!(reference);
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exploration_input_uses_only_explicit_project_data_root() {
        let temp = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(temp.path()).unwrap();
        let software = base.join("software");
        let project = base.join("project");
        std::fs::create_dir(&software).unwrap();
        std::fs::create_dir(&project).unwrap();
        std::fs::write(software.join("SKILL.md"), "test software identity").unwrap();
        std::fs::write(software.join("request.json"), b"software copy").unwrap();
        std::fs::write(project.join("request.json"), b"project input").unwrap();
        let unbound = SkillRoot::new(&software).unwrap();
        assert_eq!(
            input_reference(&unbound, Path::new("request.json"))
                .unwrap()
                .1,
            b"software copy"
        );
        let root = unbound.with_project_data_root(&project).unwrap();
        let (path, raw, reference) = input_reference(&root, Path::new("request.json")).unwrap();
        assert_eq!(path, project.join("request.json"));
        assert_eq!(raw, b"project input");
        assert_eq!(reference.path, "request.json");
        assert_eq!(reference.sha256, sha256(b"project input"));
        assert_eq!(root.path(), software);
        assert!(input_reference(&root, &software.join("request.json")).is_err());
        assert!(input_reference(&root, Path::new("../software/request.json")).is_err());
        std::fs::remove_file(project.join("request.json")).unwrap();
        assert!(input_reference(&root, Path::new("request.json")).is_err());
    }
}
