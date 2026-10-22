//! Complete book DTOs and explicit authoring maintenance; no implicit promotion.
use anyhow::Result;
use clap::Subcommand;
use oe_core::SkillRoot;
use oe_kernel::books;
use oe_semantic::book_sources;
use serde_json::{Value, json};
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum BookCommand {
    /// Observe this source/executable for task pins; build origin remains unknown.
    RuntimeIdentity,
    /// Execute supported Rust checks against all 29 original chapter oracles.
    MigrationReview {
        #[arg(long)]
        output: PathBuf,
    },
    /// Read verified historical package asset bytes without the retired runtime.
    HistoricalAsset {
        #[arg(long,value_parser=["vol1","vol2"])]
        volume: String,
        #[arg(long)]
        chapter: String,
        #[arg(long)]
        asset: String,
    },
    /// Collect complete versioned local evidence, without changing historical records.
    Evidence {
        #[command(subcommand)]
        command: Box<crate::book_release::EvidenceCommand>,
    },
    /// Create or independently replay a source-bound local candidate manifest.
    Release {
        #[command(subcommand)]
        command: Box<crate::book_release::ReleaseCommand>,
    },
    /// Read the complete native migration map, including retired entries.
    MigrationMap {
        #[arg(long, value_parser = ["vol1", "vol2"])]
        volume: String,
    },
    /// Preserve every matching successor; ambiguous or missing is not guessed.
    ResolveSuccessor {
        #[arg(long)]
        old_path: String,
        #[arg(long, value_parser = ["vol1", "vol2"])]
        volume: Option<String>,
    },
    /// Return the full verified native UTF-8 asset in JSON, without truncation.
    AssetText {
        #[arg(long)]
        package: String,
        #[arg(long)]
        asset: String,
    },
    ChapterAssetText {
        #[arg(long, value_parser = ["vol1", "vol2"])]
        volume: String,
        #[arg(long)]
        chapter: String,
        #[arg(long)]
        asset: String,
    },
    /// Native checks of both source families; retains all checks and source hashes.
    VerifySources,
    /// Engrave explicitly supplied controlled extracts into a private artifact directory.
    Engrave {
        #[arg(long)]
        source_root: PathBuf,
        #[arg(long)]
        book_root: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, value_delimiter = ',', default_value = "1,3")]
        parts: Vec<u8>,
        /// Native per-file replacement, not an atomic batch or publication approval.
        #[arg(long)]
        overwrite: bool,
    },
    /// Search checked formal sources or an explicitly selected historical corpus.
    Search {
        #[arg(required = true, num_args = 1..)]
        query: Vec<String>,
        #[arg(long, default_value = "book", value_parser = ["all", "book", "archive", "pde", "cad"])]
        scope: String,
        #[arg(long, default_value_t = 12)]
        limit: usize,
        #[arg(long, default_value_t = 2)]
        context: usize,
        #[arg(long)]
        external_root: Option<PathBuf>,
        /// Historical case text only; grants no CAD application authority.
        #[arg(long)]
        cad_root: Option<PathBuf>,
    },
    /// Check four authoring locks; --write explicitly refreshes authored byte identities.
    AuthoringLocks {
        #[arg(long)]
        write: bool,
    },
    /// Check or explicitly rebind 29 authoring packages. Book repository is global --root.
    Rebind {
        #[arg(long)]
        semantica_root: PathBuf,
        #[arg(long)]
        write: bool,
    },
}

fn source_response(command: &str, result: Value) -> Value {
    json!({"schema":"ontology-engineering.book-source-command/v1", "command":command,
        "status":if result.get("passed").is_some_and(|v| v != true) {"blocked"} else {"completed"},
        "result":result,"semantic_execution":"not_run","fact_admitted":false,"publication":"not_performed"})
}

pub fn execute(root: &SkillRoot, command: BookCommand) -> Result<u8> {
    let response = match command {
        BookCommand::RuntimeIdentity => source_response(
            "runtime-identity",
            oe_semantic::books::rust::source_execution::runtime_identity(root)?,
        ),
        BookCommand::MigrationReview { output } => {
            crate::v2::require_execution(root, "formal_chapter_migration")?;
            let record = oe_kernel::chapters::migration(root.path())?;
            oe_core::write_new(&root.private_path(&output)?, &record)?;
            source_response(
                "migration-review",
                json!({"chapter_count":record["chapters"].as_array().map(Vec::len),"receipt":output,"scope":record["scope"],"project_adoption":"not_established"}),
            )
        }
        BookCommand::HistoricalAsset {
            volume,
            chapter,
            asset,
        } => source_response(
            "historical-asset",
            oe_kernel::chapters::asset_text(root.path(), &volume, &chapter, &asset)?,
        ),
        BookCommand::Evidence { command } => {
            return crate::book_release::execute_evidence(root, *command);
        }
        BookCommand::Release { command } => {
            return crate::book_release::execute_release(root, *command);
        }
        BookCommand::MigrationMap { volume } => {
            source_response("migration-map", books::migration_map(root.path(), &volume)?)
        }
        BookCommand::ResolveSuccessor { old_path, volume } => source_response(
            "resolve-successor",
            books::resolve_successor(root.path(), &old_path, volume.as_deref())?,
        ),
        BookCommand::AssetText { package, asset } => source_response(
            "asset-text",
            books::asset_text(root.path(), &package, &asset)?,
        ),
        BookCommand::ChapterAssetText {
            volume,
            chapter,
            asset,
        } => source_response(
            "chapter-asset-text",
            oe_kernel::chapters::asset_text(root.path(), &volume, &chapter, &asset)?,
        ),
        BookCommand::VerifySources => {
            source_response("verify-sources", books::verify_sources(root.path())?)
        }
        BookCommand::Engrave {
            source_root,
            book_root,
            output,
            parts,
            overwrite,
        } => books::engrave(
            root,
            &books::EngraveOptions {
                source_root,
                book_root,
                output,
                parts,
                overwrite,
            },
        )?,
        BookCommand::Search {
            query,
            scope,
            limit,
            context,
            external_root,
            cad_root,
        } => {
            let mut response = source_response(
                "search",
                book_sources::search(
                    &oe_knowledge::continuity::book_root(root)?,
                    &query.join(" "),
                    &book_sources::SearchOptions {
                        scope: scope.clone(),
                        limit,
                        context,
                        external_root: external_root.map(|p| root.expand(&p)).transpose()?,
                        cad_root: cad_root.map(|p| root.expand(&p)).transpose()?,
                    },
                )?,
            );
            if scope == "archive" {
                response["provenance_warning"] = book_sources::ARCHIVE_WARNING.into();
            }
            response
        }
        BookCommand::AuthoringLocks { write } => {
            let book_root = if write {
                root.path().to_owned()
            } else {
                oe_knowledge::continuity::book_root(root)?
            };
            source_response(
                "authoring-locks",
                book_sources::check_authoring_locks(&book_root, write)?,
            )
        }
        BookCommand::Rebind {
            semantica_root,
            write,
        } => source_response(
            "rebind",
            book_sources::rebind(root.path(), &root.expand(&semantica_root)?, write)?,
        ),
    };
    let code = if response["status"] == "completed" {
        0
    } else {
        1
    };
    println!("{}", serde_json::to_string_pretty(&response)?);
    Ok(code)
}
