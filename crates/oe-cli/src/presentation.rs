//! Built-in source-bound engineering presentation, with existing execution guards.
use anyhow::Result;
use clap::Subcommand;
use oe_core::SkillRoot;
use oe_engineering::presentation::{bind, load_project};
use std::path::PathBuf;
#[derive(Subcommand)]
pub enum PresentationCommand {
    /// Bind an explicit source/object/condition to the current Project.
    Bind {
        #[arg(long)]
        project: PathBuf,
        #[arg(long)]
        source: PathBuf,
        #[arg(long)]
        object: String,
        #[arg(long)]
        condition: String,
        #[arg(long)]
        title: String,
        #[arg(long)]
        output: PathBuf,
    },
    /// Generate a finite-state offline HTML/SVG delivery in a new var/ directory.
    Generate {
        #[arg(long)]
        project: PathBuf,
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Recompute current source, Project and complete delivery bytes without rewriting.
    Verify {
        #[arg(long)]
        project: PathBuf,
        #[arg(long)]
        directory: PathBuf,
    },
}
pub fn run(root: &SkillRoot, command: PresentationCommand) -> Result<serde_json::Value> {
    let execution = crate::v2::require_execution(root, "engineering_presentation")?;
    let result = match command {
        PresentationCommand::Bind {
            project,
            source,
            object,
            condition,
            title,
            output,
        } => {
            let project = load_project(root, &project)?;
            let request = bind(root, &project, &source, &object, &condition, &title)?;
            let scene = oe_engineering::presentation::prepare(root, &project, &request)?;
            oe_docs::presentation::validate_scene(&scene)?;
            oe_core::write_new(
                &root.private_path(&output)?,
                &serde_json::to_value(&request)?,
            )?;
            serde_json::json!({"status":"bound","request":request,"output":output})
        }
        PresentationCommand::Generate {
            project,
            input,
            output,
        } => {
            let project = load_project(root, &project)?;
            let bytes = oe_core::read_regular_bytes(&root.expand(&input)?, 4 * 1024 * 1024)?;
            let request = serde_json::from_value(oe_core::json::strict_json(&bytes)?)?;
            let receipt = oe_docs::presentation::generate(root, &project, &request, &output)?;
            serde_json::json!({"status":"generated","directory":output,"receipt":receipt})
        }
        PresentationCommand::Verify { project, directory } => {
            let project = load_project(root, &project)?;
            let receipt = oe_docs::presentation::verify(root, &project, &directory)?;
            serde_json::json!({"status":"current","directory":directory,"receipt":receipt})
        }
    };
    Ok(
        serde_json::json!({"result":result,"execution_binding":execution,"engineering_adoption":"not_established"}),
    )
}
