//! Explicit project storage adapter; the application owner retains all semantics.
use anyhow::Result;
use clap::Subcommand;
use oe_core::SkillRoot;
use oe_engineering::project_workspace as owner;
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
pub enum WorkspaceCommand {
    Init {
        #[arg(long)]
        workspace_id: String,
        #[arg(long)]
        project_id: String,
    },
    Commit {
        #[arg(long)]
        input: PathBuf,
    },
    Resume {
        /// Write a new derived Markdown view; current source data is always rechecked.
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Begin {
        /// ArtifactReference JSON for the exact current commit, obtained from resume.
        #[arg(long)]
        expected_head: PathBuf,
        #[arg(long)]
        stage: String,
    },
    LearningGoal {
        #[arg(long)]
        output: PathBuf,
    },
    LearningCheck {
        /// ArtifactReference JSON returned as binding by workspace learning-freeze.
        #[arg(long)]
        input: PathBuf,
    },
    LearningFreeze {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
}
fn read(root: &SkillRoot, path: &Path) -> Result<Value> {
    let path = root.private_path(path)?;
    Ok(oe_core::json::strict_json(&oe_core::read_regular_bytes(
        &path,
        16 * 1024 * 1024,
    )?)?)
}
pub fn execute(root: &SkillRoot, command: WorkspaceCommand) -> Result<u8> {
    // Resume and begin are source-data checks. They must work after a fresh
    // process starts without turning historical execution into current approval.
    let value = match command {
        WorkspaceCommand::Init {
            workspace_id,
            project_id,
        } => {
            crate::v2::require_execution(root, "workspace_initialize")?;
            owner::initialize(root, &workspace_id, &project_id)?
        }
        WorkspaceCommand::Commit { input } => {
            crate::v2::require_execution(root, "workspace_commit")?;
            owner::commit(root, &serde_json::from_value(read(root, &input)?)?)?
        }
        WorkspaceCommand::Resume { output } => match output {
            Some(output) => owner::render_resume(root, &output)?,
            None => owner::resume(root)?,
        },
        WorkspaceCommand::Begin {
            expected_head,
            stage,
        } => owner::begin(
            root,
            &serde_json::from_value(read(root, &expected_head)?)?,
            &stage,
        )?,
        WorkspaceCommand::LearningGoal { output } => owner::learning_goal(root, &output)?,
        WorkspaceCommand::LearningCheck { input } => {
            owner::learning_check(root, &serde_json::from_value(read(root, &input)?)?)?
        }
        WorkspaceCommand::LearningFreeze { input, output } => {
            crate::v2::require_execution(root, "workspace_learning_freeze")?;
            owner::learning_freeze(root, &read(root, &input)?, &output)?
        }
    };
    let failed = matches!(
        value["status"].as_str(),
        Some("conflict" | "incomplete" | "needs_review" | "effect_unknown")
    );
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(if failed { 1 } else { 0 })
}
