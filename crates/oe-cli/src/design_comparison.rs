use anyhow::{Result, ensure};
use clap::Subcommand;
use oe_core::{SkillRoot, read_regular_bytes, sha256};
use serde_json::{Value, json};
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum DesignComparisonCommand {
    /// Freeze one Project, parent task, complete attempts and controlled model rules.
    Freeze {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Recheck the complete frozen source closure without executing work.
    Verify {
        #[arg(long)]
        frozen: PathBuf,
    },
    /// Execute both original attempts and compare actual declared-model results.
    Evaluate {
        #[arg(long)]
        frozen: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[command(flatten)]
        tools: crate::KnowledgeTools,
    },
}
pub fn run(root: &SkillRoot, command: DesignComparisonCommand) -> Result<Value> {
    let binding = crate::v2::require_execution(root, "design_comparison")?;
    let input = match &command {
        DesignComparisonCommand::Freeze { input, .. } => input,
        DesignComparisonCommand::Verify { frozen }
        | DesignComparisonCommand::Evaluate { frozen, .. } => frozen,
    };
    let path = root.expand(input)?;
    ensure!(
        path.starts_with(root.path()),
        "design_comparison_input_outside_root"
    );
    let raw = read_regular_bytes(&path, 16 * 1024 * 1024)?;
    let reference = oe_contracts::ArtifactReference {
        path: oe_core::portable_source_path(root.path(), &path)?,
        sha256: sha256(&raw),
    };
    let completion = match &command {
        DesignComparisonCommand::Freeze { output, .. } => Some((output.clone(), "freeze")),
        DesignComparisonCommand::Evaluate { output, .. } => Some((output.clone(), "evaluate")),
        _ => None,
    };
    let mut result = match command {
        DesignComparisonCommand::Freeze { output, .. } => {
            oe_engineering::design_comparison::freeze(root, &reference, &output)?
        }
        DesignComparisonCommand::Verify { .. } => {
            oe_engineering::design_comparison::verify(root, &reference)?
        }
        DesignComparisonCommand::Evaluate { output, tools, .. } => {
            oe_engineering::design_comparison::evaluate(
                root,
                &reference,
                &output,
                &tools.resolve()?,
            )?
        }
    };
    ensure!(
        read_regular_bytes(&path, 16 * 1024 * 1024)? == raw,
        "design_comparison_cli_input_changed"
    );
    if let Some((output, operation)) = completion {
        let dir = root.expand(&output)?;
        let core = read_regular_bytes(&dir.join("completed.json"), 16 * 1024 * 1024)?;
        oe_core::write_new_bytes(&dir.join("cli-input.json"), &raw)?;
        oe_core::write_new(&dir.join("execution-binding.json"), &binding)?;
        let identity = read_regular_bytes(&dir.join("execution-binding.json"), 16 * 1024 * 1024)?;
        let final_binding = crate::v2::require_execution(root, "design_comparison_completion")?;
        for key in [
            "normalizer",
            "cli",
            "semantic_contracts_sha256",
            "executable_sha256",
        ] {
            ensure!(
                final_binding[key] == binding[key],
                "design_comparison_execution_identity_changed:{key}"
            );
        }
        oe_core::write_new(&dir.join("execution-recheck.json"), &final_binding)?;
        let recheck = read_regular_bytes(&dir.join("execution-recheck.json"), 16 * 1024 * 1024)?;
        ensure!(
            read_regular_bytes(&path, 16 * 1024 * 1024)? == raw
                && read_regular_bytes(&dir.join("completed.json"), 16 * 1024 * 1024)? == core,
            "design_comparison_cli_completion_changed"
        );
        let terminal = json!({"schema":"oev2.design-comparison-cli-completed/1","status":"completed","operation":operation,
            "input":reference,"core_completed":{"path":"completed.json","sha256":sha256(core)},
            "input_copy":{"path":"cli-input.json","sha256":sha256(&raw)},
            "execution_binding":{"path":"execution-binding.json","sha256":sha256(identity)},
            "execution_recheck":{"path":"execution-recheck.json","sha256":sha256(recheck)},
            "design_complete":false,"adopted":false,"project_mutated":false});
        oe_core::write_new(&dir.join("cli-completed.json"), &terminal)?;
        result["cli_completion"] = terminal;
    }
    result["execution_binding"] = binding;
    Ok(result)
}
