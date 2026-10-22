//! Supplier evidence belongs to the current project; selection is not acceptance.
use anyhow::{Context, Result, ensure};
use clap::{Args, Subcommand};
use oe_browser::{BackendKind, BrowserTask, SnapshotSource, backend};
use oe_cad::supplier::{archive, artifacts, session, source};
use oe_core::{
    SkillRoot, create_new_private_dir, fingerprint_file, read_regular_bytes, sha256, write_new,
    write_new_bytes,
};
use serde_json::{Value, json};
use std::path::PathBuf;

#[derive(Args)]
pub struct ConnectionArgs {
    /// Explicit existing Peekaboo background page and observed action scope.
    #[arg(long)]
    task: PathBuf,
    #[arg(long)]
    backend_config: PathBuf,
    #[arg(long)]
    output: PathBuf,
}

#[derive(Subcommand)]
pub enum SupplierCommand {
    /// Check exact supplied fields on the normal account page; never log in.
    SessionCheck {
        #[command(flatten)]
        connection: ConnectionArgs,
        #[arg(long)]
        identity: PathBuf,
        /// Explicitly navigate this bound tab once to the fixed account page.
        #[arg(long)]
        navigate_account: bool,
        #[arg(long, default_value_t = 40)]
        max_seconds: u64,
    },
    /// Inspect and retain an existing file snapshot; no fresh download is implied.
    Inspect {
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        part: String,
        #[arg(long)]
        format: String,
        #[arg(long)]
        output: PathBuf,
        /// Apply the retired AP203 comparison's byte contract for this local ZIP only.
        #[arg(long)]
        historical_ap203: bool,
    },
    /// Download an explicitly observed public MISUMI 2D archive into this skill's var/.
    Archive {
        #[arg(long)]
        url: String,
        #[arg(long)]
        observed_from: String,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        expected_sha256: Option<String>,
    },
    /// Historical AP203 comparison only: transfer an exact already observed URL.
    DownloadObservedAp203 {
        #[arg(long)]
        url: String,
        #[arg(long)]
        part: String,
        #[arg(long)]
        output: PathBuf,
    },
}

fn envelope(command: &str, result: Value, code: u8) -> (Value, u8) {
    (
        json!({"schema":"ontology-engineering.native-supplier-command/v1","command":command,
        "status":result["status"],"result":result,"semantic_execution":"not_run",
        "engineering_acceptance":"not_evaluated","publication":"not_performed",
        "fact_admitted":false,"purchasing":"not_performed"}),
        code,
    )
}

struct Connection {
    task: BrowserTask,
    observer: SnapshotSource,
    config: backend::BackendConfig,
    source: Value,
    output: PathBuf,
    inputs: Value,
    task_value: Value,
    config_value: Value,
}
impl Connection {
    fn prepare(root: &SkillRoot, args: ConnectionArgs) -> Result<Self> {
        let task_input = crate::browser::input(root, &args.task)?;
        let task = BrowserTask::parse(task_input.value.clone())?;
        ensure!(
            task.backend == BackendKind::PeekabooBackground
                && task.tab_id.is_some()
                && task.keep_open,
            "supplier_requires_explicit_peekaboo_background_page"
        );
        ensure!(
            oe_browser::task::http_origin(&task.url)? == session::ORIGIN,
            "supplier_misumi_origin_required"
        );
        let config_input = crate::browser::input(root, &args.backend_config)?;
        let output = root.private_path(&args.output)?;
        ensure!(!output.exists(), "supplier_output_already_exists");
        let config = crate::browser::config(root, &config_input, &task, output.join("backend"))?;
        Ok(Self {
            task,
            observer: oe_browser::source::snapshot(root)?,
            source: source::identity(root)?,
            config,
            output,
            inputs: json!({"task":task_input.reference,"backend_config":config_input.reference}),
            task_value: task_input.value,
            config_value: config_input.value,
        })
    }
    fn connect(
        &self,
        root: &SkillRoot,
    ) -> Result<std::result::Result<Box<dyn oe_browser::Browser>, ()>> {
        source::ensure_current(root, &self.source)?;
        root.new_private_dir(&self.output)?;
        write_new(&self.output.join("source-identity.json"), &self.source)?;
        write_new(&self.output.join("inputs.json"), &self.inputs)?;
        write_new(&self.output.join("task.json"), &self.task_value)?;
        write_new(&self.output.join("backend-config.json"), &self.config_value)?;
        Ok(backend::connect(root, &self.task, &self.observer, &self.config).map_err(|_| ()))
    }
    fn finish(&self, root: &SkillRoot, mut result: Value) -> Result<Value> {
        let current = source::ensure_current(root, &self.source).is_ok();
        result["source_current"] = json!(current);
        result["tab"] = json!({"status":"retained","owns_target":false});
        if !current {
            result["requested_status"] = result["status"].clone();
            result["status"] = json!("source_identity_changed");
            result["account_verified"] = json!(false);
        }
        write_new(&self.output.join("command-result.json"), &result)?;
        Ok(result)
    }
    fn connection_failed(&self, root: &SkillRoot) -> Result<Value> {
        self.finish(
            root,
            json!({"status":"browser_connection_required","account_verified":false,
            "model_calls":0,"generation":"not_dispatched","verification":"not_run",
            "engineering_acceptance":"not_evaluated","backend_evidence":"backend/"}),
        )
    }
}

pub fn run(root: &SkillRoot, command: SupplierCommand) -> Result<(Value, u8)> {
    match command {
        SupplierCommand::SessionCheck {
            connection,
            identity,
            navigate_account,
            max_seconds,
        } => {
            ensure!(
                (1..=120).contains(&max_seconds),
                "session_check_budget_out_of_range"
            );
            let identity = root.private_path(&identity)?;
            session::read_identity(&identity)?;
            let connection = Connection::prepare(root, connection)?;
            let mut browser = match connection.connect(root)? {
                Ok(browser) => browser,
                Err(()) => {
                    return Ok(envelope(
                        "session-check",
                        connection.connection_failed(root)?,
                        1,
                    ));
                }
            };
            let result = match session::check(
                root,
                browser.as_mut(),
                &identity,
                &connection.output.join("session"),
                &session::SessionOptions {
                    navigate_account,
                    max_seconds,
                },
            ) {
                Ok(value) => value,
                Err(_) => json!({"status":"account_check_unconfirmed","account_verified":false,
                    "verification":"not_run","retained_operation_evidence":"session/"}),
            };
            let result = connection.finish(root, result)?;
            let code = if result["account_verified"] == true {
                0
            } else {
                1
            };
            Ok(envelope("session-check", result, code))
        }
        SupplierCommand::Inspect {
            file,
            part,
            format,
            output,
            historical_ap203,
        } => {
            let source = root.expand(&file)?;
            let name = source
                .file_name()
                .and_then(|v| v.to_str())
                .context("supplier_file_name_required")?;
            let before = fingerprint_file(&source, artifacts::LIMIT as u64)?;
            let bytes = read_regular_bytes(&source, artifacts::LIMIT)?;
            ensure!(
                bytes.len() as u64 == before.bytes && sha256(&bytes) == before.sha256,
                "supplier_file_changed_during_snapshot"
            );
            let inspected = if historical_ap203 {
                ensure!(
                    format.eq_ignore_ascii_case("STEP AP203"),
                    "historical_inspection_requires_explicit_ap203_format"
                );
                ensure!(
                    name.to_ascii_lowercase().ends_with(".zip"),
                    "historical_ap203_zip_required"
                );
                artifacts::inspect_ap203_archive(bytes, &part)?
            } else {
                artifacts::inspect_download(name, bytes, &part, &format)?
            };
            ensure!(
                fingerprint_file(&source, artifacts::LIMIT as u64)? == before,
                "supplier_file_changed_during_inspection"
            );
            let output = root.new_private_dir(&output)?;
            let original = output.join("source");
            create_new_private_dir(&original)?;
            write_new_bytes(&original.join(name), &inspected.blob)?;
            let members = output.join("members");
            create_new_private_dir(&members)?;
            for (name, bytes) in &inspected.files {
                write_new_bytes(&members.join(name), bytes)?;
            }
            let result = json!({"status":"inspected", "artifact":inspected.receipt,
                "input_stage":"existing_file_snapshot","fresh_download_observed":false,
                "source_sha256":before.sha256,"source_bytes":before.bytes,
                "historical_ap203_comparison":historical_ap203,"output":output.strip_prefix(root.path())?,
                "native_handoff":{"part":part,"requested_format":format,
                    "native_import":"not_run","native_save":"not_run","independent_reopen":"not_run",
                    "geometry_and_units":"requires_native_context_readback"},
                "model_calls":0,"browser_actions":0,"engineering_acceptance":"not_evaluated"});
            write_new(&output.join("receipt.json"), &result)?;
            Ok(envelope("inspect", result, 0))
        }
        SupplierCommand::Archive {
            url,
            observed_from,
            output,
            expected_sha256,
        } => {
            let result = archive::download(
                root,
                &url,
                &observed_from,
                &output,
                expected_sha256.as_deref(),
            )?;
            let code = if result["status"] == "artifact_acquired" {
                0
            } else {
                1
            };
            Ok(envelope("archive", result, code))
        }
        SupplierCommand::DownloadObservedAp203 { url, part, output } => {
            let result = archive::download_observed_ap203(root, &url, &part, &output)?;
            let code = if result["status"] == "artifact_acquired" {
                0
            } else {
                1
            };
            Ok(envelope("download-observed-ap203", result, code))
        }
    }
}
