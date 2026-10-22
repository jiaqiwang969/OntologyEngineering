//! Local evidence and candidate replay remain separate from publication authority.
use anyhow::Result;
use clap::{Args, Subcommand};
use oe_core::SkillRoot;
use oe_semantic::{
    book_release,
    release_evidence::{self, EvidenceOptions},
};
use serde_json::{Value, json};
use std::path::PathBuf;

#[derive(Args)]
pub struct EvidenceArgs {
    /// New private ROOT/var directory for complete raw observations and failures.
    #[arg(long)]
    output: PathBuf,
    #[arg(long, default_value = "git")]
    git: PathBuf,
    #[arg(long, default_value = "runtime/.venv/bin/python")]
    python: PathBuf,
    #[arg(long, default_value = "cargo")]
    cargo: PathBuf,
    #[arg(long, default_value = "pdfinfo")]
    pdfinfo: PathBuf,
    #[arg(long, default_value = "pdffonts")]
    pdffonts: PathBuf,
    #[arg(long, default_value = "pdftotext")]
    pdftotext: PathBuf,
    #[arg(long)]
    semantica_root: Option<PathBuf>,
    /// Existing source-bound visual review; the collector never invents a review.
    #[arg(long)]
    visual_review: Option<PathBuf>,
}
impl EvidenceArgs {
    fn options(self, root: &SkillRoot, commit: Option<String>) -> Result<EvidenceOptions> {
        let mut result = EvidenceOptions::new(self.output);
        result.git = self.git;
        result.python = self.python;
        result.cargo = self.cargo;
        result.pdfinfo = self.pdfinfo;
        result.pdffonts = self.pdffonts;
        result.pdftotext = self.pdftotext;
        result.semantica_root = self.semantica_root.map(|p| root.expand(&p)).transpose()?;
        result.visual_review = self.visual_review.map(|p| root.expand(&p)).transpose()?;
        result.oe_source_commit = commit;
        Ok(result)
    }
}

#[derive(Subcommand)]
pub enum EvidenceCommand {
    /// Initialize pending records; existing valid records are retained.
    Governance {
        /// Explicitly reset both current v2 records to pending; never grants approval.
        #[arg(long)]
        reset_existing: bool,
    },
    /// Check authoring locks, installed runtime identity and candidate privacy.
    Static(EvidenceArgs),
    /// Re-run native checks and retain full source/condition/failed binding findings.
    BookBindings(EvidenceArgs),
    /// Run the owned Cargo workspace and the external Semantica Python suites.
    Regressions {
        #[arg(long)]
        oe_source_commit: String,
        #[command(flatten)]
        tools: EvidenceArgs,
    },
    /// Inspect both PDFs and bind the supplied visual review with full page identities.
    PdfQa(EvidenceArgs),
}

#[derive(Subcommand)]
pub enum ReleaseCommand {
    /// Freshly replay every evidence family and publish only a local candidate.
    Create {
        #[arg(long)]
        oe_source_commit: String,
        #[command(flatten)]
        tools: EvidenceArgs,
        /// Always rejected: technical evidence cannot authorize publication.
        #[arg(long)]
        claim_release: bool,
    },
    /// Recheck every source and perform a new replay without changing stored evidence.
    Verify {
        #[arg(long, default_value = book_release::MANIFEST_PATH)]
        manifest: String,
        #[command(flatten)]
        tools: EvidenceArgs,
    },
}

fn print_result(command: &str, result: Value) -> Result<u8> {
    let passed = command_completed(&result);
    let report = json!({"schema":"ontology-engineering.book-evidence-command/v2","command":command,"status":if passed {"completed"} else {"blocked"},"result":result,"publication":"not_performed","fact_admitted":false});
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(if passed { 0 } else { 1 })
}

fn command_completed(result: &Value) -> bool {
    match result.get("passed") {
        Some(value) => value == true,
        None => result.as_object().is_some_and(|documents| {
            !documents.is_empty()
                && documents
                    .values()
                    .all(|document| document["passed"] == true)
        }),
    }
}

pub fn execute_evidence(root: &SkillRoot, command: EvidenceCommand) -> Result<u8> {
    let (name, report) = match command {
        EvidenceCommand::Governance { reset_existing } => (
            "evidence.governance",
            book_release::initialize_governance(root, reset_existing)?,
        ),
        EvidenceCommand::Static(tools) => (
            "evidence.static",
            release_evidence::collect_static(root, &tools.options(root, None)?)?,
        ),
        EvidenceCommand::BookBindings(tools) => (
            "evidence.book-bindings",
            release_evidence::collect_book_bindings(root, &tools.options(root, None)?)?,
        ),
        EvidenceCommand::Regressions {
            oe_source_commit,
            tools,
        } => (
            "evidence.regressions",
            release_evidence::collect_regressions(
                root,
                &tools.options(root, Some(oe_source_commit))?,
            )?,
        ),
        EvidenceCommand::PdfQa(tools) => (
            "evidence.pdf-qa",
            release_evidence::collect_pdf_qa(root, &tools.options(root, None)?)?,
        ),
    };
    print_result(name, report)
}

pub fn execute_release(root: &SkillRoot, command: ReleaseCommand) -> Result<u8> {
    let (name, report) = match command {
        ReleaseCommand::Create {
            oe_source_commit,
            tools,
            claim_release,
        } => {
            let evidence = tools.options(root, Some(oe_source_commit.clone()))?;
            (
                "release.create",
                book_release::create_manifest(
                    root,
                    &book_release::CreateOptions {
                        oe_source_commit,
                        evidence,
                        claim_release,
                    },
                )?,
            )
        }
        ReleaseCommand::Verify { manifest, tools } => (
            "release.verify",
            book_release::verify_manifest(
                root,
                &book_release::VerifyOptions {
                    manifest,
                    evidence: tools.options(root, None)?,
                },
            )?,
        ),
    };
    print_result(name, report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collection_exit_status_includes_each_document_and_preserves_blocked() {
        assert!(command_completed(&json!({"pdf_qa":{"passed":true}})));
        assert!(command_completed(
            &json!({"authoring_locks":{"passed":true},"runtime_identity":{"passed":true},"privacy":{"passed":true}})
        ));
        assert!(!command_completed(
            &json!({"authoring_locks":{"passed":true},"runtime_identity":{"passed":false},"privacy":{"passed":true}})
        ));
        assert!(!command_completed(&json!({"privacy":{}})));
        assert!(!command_completed(&json!({})));
        assert!(command_completed(
            &json!({"passed":true,"release_eligible":false})
        ));
        assert!(!command_completed(&json!({"passed":false})));
    }
}
