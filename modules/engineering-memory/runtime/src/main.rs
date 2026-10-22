use anyhow::{Result, ensure};
use clap::{Parser, Subcommand};
use engineering_memory_runtime::contracts::SyncBudget;
use engineering_memory_runtime::{
    Request, capabilities, evidence, execute,
    store::{MAX_BYTES, Store, read_json},
};
use serde_json::{Value, json};
use std::io::Read;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    version,
    about = "Shared source-bound engineering memory; integrated Rust module"
)]
struct Cli {
    /// Shared data directory, independent of profile and working directory.
    #[arg(long, global = true)]
    home: Option<PathBuf>,
    #[command(subcommand)]
    command: Operation,
}

#[derive(Subcommand)]
enum Operation {
    /// Organize a bounded task using an internal method; action is checked by the module.
    Organization {
        /// Includes read-only frontier and inspect-claim for existing work and evidence.
        action: String,
        #[arg(long)]
        input: Option<PathBuf>,
    },
    Capabilities,
    /// Client lifecycle hook. Returns native Codex/Claude hook JSON on stdout.
    Hook {
        #[arg(long)]
        policy: PathBuf,
        #[arg(long, value_parser=["codex", "claude"])]
        provider: String,
        #[arg(long)]
        profile: Option<String>,
    },
    /// Register queued client transcripts; subsequent run/sync performs intake.
    HookIntake {
        #[arg(long, default_value_t = 8)]
        max_sources: usize,
        #[arg(long, default_value_t = 536_870_912)]
        max_verify_bytes: u64,
    },
    /// Task-scoped memory context, Project handoff and actual-use feedback.
    Session {
        #[command(subcommand)]
        command: SessionOperation,
    },
    #[command(alias = "engineering-evidence")]
    Evidence(evidence::EvidenceArgs),
    /// One JSON request from stdin, or --input; stdout is one JSON response.
    Rpc {
        #[arg(long)]
        input: Option<PathBuf>,
    },
    Status,
    Pending {
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    Packet {
        #[arg(long)]
        id: String,
    },
    /// Verify native source identity and admission without printing source text.
    SourceEligibility {
        #[arg(long)]
        packet: String,
    },
    Record {
        #[arg(long)]
        input: PathBuf,
    },
    Feedback {
        #[arg(long)]
        input: PathBuf,
    },
    Compare {
        #[arg(long)]
        input: PathBuf,
    },
    Resolve {
        #[arg(long)]
        input: PathBuf,
    },
    ConfigureExtractor {
        #[arg(long)]
        input: PathBuf,
    },
    Register {
        #[arg(long)]
        input: PathBuf,
        #[arg(long, default_value_t = 536_870_912)]
        max_verify_bytes: u64,
    },
    /// Link a pristine duplicate registration to the existing scan owner.
    AliasSource {
        #[arg(long)]
        input: PathBuf,
    },
    /// Capture one fixed historical version using native clone; never copies the whole file.
    Capture {
        #[arg(long)]
        input: PathBuf,
    },
    Get {
        #[arg(long)]
        id: String,
        #[arg(long)]
        revision: Option<u64>,
    },
    Verify {
        #[arg(long)]
        id: String,
        #[arg(long)]
        revision: Option<u64>,
    },
    Recall {
        #[arg(default_value = "")]
        query: String,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        condition: Vec<String>,
        #[arg(long, default_value_t = 5)]
        limit: usize,
        #[arg(long)]
        include_other_projects: bool,
    },
    Handoff {
        #[arg(long)]
        id: String,
        #[arg(long)]
        revision: u64,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        condition: Vec<String>,
    },
    Sync {
        #[command(flatten)]
        budget: BudgetArgs,
    },
    Run {
        #[command(flatten)]
        budget: BudgetArgs,
        #[arg(long, default_value_t = 1)]
        max_packets: usize,
    },
    /// One bounded background cycle; does not install a timer.
    Tick {
        #[arg(long)]
        policy: PathBuf,
    },
    // --- track D: history anchor catalog ---
    /// Build or extend the visible-conversation anchor catalog; bounded, resumable, no model calls.
    HistoryIndexBuild {
        #[arg(long)]
        input: PathBuf,
    },
    /// Read-only catalog status.
    HistoryIndexStatus,
    /// Read-only ranked search over committed catalog anchors; re-read hits before citing.
    HistorySearch {
        #[arg(long)]
        input: PathBuf,
    },
}

#[derive(Subcommand)]
enum SessionOperation {
    /// Prepare native v2 intakes and a complete method candidate; no Project write.
    V2Prepare {
        #[arg(long)]
        input: PathBuf,
    },
    /// Recheck original provenance and the actual receiving Project through v2.
    V2Verify {
        #[arg(long)]
        input: PathBuf,
    },
    /// Export all selected original messages and their original/derived positions.
    Export {
        #[arg(long)]
        input: PathBuf,
    },
    /// Recheck originals, derived bytes, latest experience and current Project.
    ExportVerify {
        #[arg(long)]
        id: String,
    },
    Prepare {
        #[arg(long)]
        input: PathBuf,
    },
    Get {
        #[arg(long)]
        id: String,
    },
    Verify {
        #[arg(long)]
        id: String,
    },
    Activate {
        #[arg(long)]
        id: String,
        #[arg(long)]
        context_sha256: String,
    },
    Handoff {
        #[arg(long)]
        context_id: String,
        #[arg(long)]
        context_sha256: String,
        #[arg(long)]
        id: String,
        #[arg(long)]
        revision: u64,
    },
    Feedback {
        #[arg(long)]
        input: PathBuf,
    },
}

#[derive(clap::Args)]
struct BudgetArgs {
    #[arg(long, default_value_t = 8_388_608)]
    max_bytes: u64,
    #[arg(long, default_value_t = 536_870_912)]
    max_verify_bytes: u64,
    #[arg(long, default_value_t = 8)]
    max_windows: u64,
    #[arg(long, default_value_t = 30)]
    max_seconds: u64,
}
impl From<BudgetArgs> for SyncBudget {
    fn from(b: BudgetArgs) -> Self {
        Self {
            max_bytes: b.max_bytes,
            max_verify_bytes: b.max_verify_bytes,
            max_windows: b.max_windows,
            max_seconds: b.max_seconds,
        }
    }
}

fn run(cli: Cli) -> Result<Value> {
    let request = match cli.command {
        Operation::Organization { action, input } => Request::Organization {
            action,
            input: match input {
                Some(path) => read_json(&path)?,
                None => json!({}),
            },
        },
        Operation::AliasSource { input } => Request::AliasSource {
            request: read_json(&input)?,
        },
        Operation::Capabilities => return Ok(capabilities()),
        Operation::Hook {
            policy,
            provider,
            profile,
        } => {
            let mut bytes = Vec::new();
            std::io::stdin().take(262_145).read_to_end(&mut bytes)?;
            ensure!(bytes.len() <= 262_144, "hook_event_budget");
            let store = Store::new(
                cli.home
                    .ok_or_else(|| anyhow::anyhow!("explicit_shared_home_required"))?,
            );
            return store.client_hook(
                &read_json(&policy)?,
                &provider,
                profile.as_deref(),
                &serde_json::from_slice(&bytes)?,
            );
        }
        Operation::HookIntake {
            max_sources,
            max_verify_bytes,
        } => Request::HookIntake {
            max_sources,
            max_verify_bytes,
        },
        Operation::Session { command } => match command {
            SessionOperation::V2Prepare { input } => Request::V2Prepare {
                request: read_json(&input)?,
            },
            SessionOperation::V2Verify { input } => Request::V2Verify {
                request: read_json(&input)?,
            },
            SessionOperation::Export { input } => Request::SessionExport {
                request: read_json(&input)?,
            },
            SessionOperation::ExportVerify { id } => Request::SessionExportVerify { id },
            SessionOperation::Prepare { input } => Request::SessionPrepare {
                request: read_json(&input)?,
            },
            SessionOperation::Get { id } => Request::SessionGet { id },
            SessionOperation::Verify { id } => Request::SessionVerify { id },
            SessionOperation::Activate { id, context_sha256 } => {
                Request::SessionActivate { id, context_sha256 }
            }
            SessionOperation::Handoff {
                context_id,
                context_sha256,
                id,
                revision,
            } => Request::SessionHandoff {
                context_id,
                context_sha256,
                id,
                revision,
            },
            SessionOperation::Feedback { input } => Request::SessionFeedback {
                input: read_json(&input)?,
            },
        },
        Operation::Evidence(args) => {
            return Ok(serde_json::to_value(evidence::read_window(&args)?)?);
        }
        Operation::Rpc { input } => {
            if let Some(input) = input {
                read_json(&input)?
            } else {
                let mut bytes = Vec::new();
                std::io::stdin()
                    .take(MAX_BYTES + 1)
                    .read_to_end(&mut bytes)?;
                ensure!(bytes.len() as u64 <= MAX_BYTES, "request_budget_exceeded");
                serde_json::from_slice(&bytes)?
            }
        }
        Operation::Status => Request::Status,
        Operation::Pending { limit } => Request::Pending { limit },
        Operation::Packet { id } => Request::Packet { id },
        Operation::SourceEligibility { packet } => Request::SourceEligibility { packet_id: packet },
        Operation::Record { input } => Request::Record {
            record: read_json(&input)?,
        },
        Operation::Feedback { input } => Request::Feedback {
            feedback: read_json(&input)?,
        },
        Operation::Compare { input } => Request::Compare {
            comparison: read_json(&input)?,
        },
        Operation::Resolve { input } => Request::Resolve {
            resolution: read_json(&input)?,
        },
        Operation::ConfigureExtractor { input } => Request::ConfigureExtractor {
            config: read_json(&input)?,
        },
        Operation::Register {
            input,
            max_verify_bytes,
        } => Request::Register {
            registration: read_json(&input)?,
            max_verify_bytes,
        },
        Operation::Capture { input } => Request::CaptureSource {
            request: read_json(&input)?,
        },
        Operation::Get { id, revision } => Request::Get { id, revision },
        Operation::Verify { id, revision } => Request::Verify { id, revision },
        Operation::Recall {
            query,
            project,
            condition,
            limit,
            include_other_projects,
        } => Request::Recall {
            query,
            project,
            conditions: condition,
            limit,
            include_other_projects,
        },
        Operation::Handoff {
            id,
            revision,
            project,
            condition,
        } => Request::Handoff {
            id,
            revision,
            project,
            conditions: condition,
        },
        Operation::Sync { budget } => Request::Sync {
            budget: budget.into(),
        },
        Operation::Run {
            budget,
            max_packets,
        } => Request::Run {
            budget: budget.into(),
            max_packets,
        },
        Operation::Tick { policy } => Request::Tick {
            policy: read_json(&policy)?,
        },
        // --- track D: history anchor catalog ---
        Operation::HistoryIndexBuild { input } => Request::HistoryIndexBuild {
            request: read_json(&input)?,
        },
        Operation::HistoryIndexStatus => Request::HistoryIndexStatus,
        Operation::HistorySearch { input } => Request::HistorySearch {
            request: read_json(&input)?,
        },
    };
    let home = cli
        .home
        .ok_or_else(|| anyhow::anyhow!("explicit_shared_home_required"))?;
    execute(&Store::new(home), request)
}

fn main() {
    let cli = Cli::parse();
    let is_hook = matches!(&cli.command, Operation::Hook { .. });
    match run(cli) {
        Ok(value) if is_hook => println!("{value}"),
        Ok(value) => println!("{}", json!({"ok":true,"data":value})),
        Err(error) if is_hook => {
            // Optional memory must not stop the user's work. This warning makes
            // failed context preparation visible instead of claiming delivery.
            println!(
                "{}",
                json!({"continue":true,"systemMessage":format!("engineering-memory 未提供本次背景：{error:#}")})
            );
        }
        Err(error) => {
            println!("{}", json!({"ok":false,"error":format!("{error:#}")}));
            std::process::exit(1);
        }
    }
}
