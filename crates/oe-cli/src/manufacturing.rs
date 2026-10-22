//! Source-bound report commands; PDF compilation is explicit and bounded.
use anyhow::Result;
use clap::Subcommand;
use oe_core::SkillRoot;
use oe_engineering::manufacturing_report::pdf::PdfTools;
use oe_engineering::manufacturing_report::publication;
use serde_json::Value;
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum ManufacturingCommand {
    /// Create or verify a captured manufacturing report.
    Report {
        #[command(subcommand)]
        command: ReportCommand,
    },
}

#[derive(Subcommand)]
pub enum ReportCommand {
    /// Publish the explicit snapshot and its views into a new var/ directory.
    Generate {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
        /// Compile and check a PDF with XeLaTeX and Poppler.
        #[arg(long)]
        compile: bool,
        #[command(flatten)]
        tools: DocumentTools,
    },
    /// Recompute and verify a captured delivery without rewriting it.
    Verify {
        #[arg(long)]
        directory: PathBuf,
        #[command(flatten)]
        tools: DocumentTools,
    },
}

#[derive(clap::Args, Default)]
pub struct DocumentTools {
    #[arg(long)]
    xelatex: Option<PathBuf>,
    #[arg(long)]
    pdftotext: Option<PathBuf>,
    #[arg(long)]
    pdffonts: Option<PathBuf>,
    #[arg(long)]
    pdfinfo: Option<PathBuf>,
}

impl DocumentTools {
    fn resolve(self) -> PdfTools {
        let mut result = PdfTools::default();
        if let Some(path) = self.xelatex {
            result.xelatex = path;
        }
        if let Some(path) = self.pdftotext {
            result.pdftotext = path;
        }
        if let Some(path) = self.pdffonts {
            result.pdffonts = path;
        }
        if let Some(path) = self.pdfinfo {
            result.pdfinfo = path;
        }
        result
    }
}

pub fn run(root: &SkillRoot, command: ManufacturingCommand) -> Result<(Value, u8)> {
    let ManufacturingCommand::Report { command } = command;
    let result = match command {
        ReportCommand::Generate {
            input,
            output,
            compile,
            tools,
        } => {
            let tools = tools.resolve();
            publication::generate_with_tools(root, &input, &output, compile.then_some(&tools))?
        }
        ReportCommand::Verify { directory, tools } => {
            publication::verify_with_tools(root, &directory, &tools.resolve())?
        }
    };
    let code = if result["passed"] == true { 0 } else { 1 };
    Ok((result, code))
}
