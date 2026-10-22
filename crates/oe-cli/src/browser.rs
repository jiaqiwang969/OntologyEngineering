//! Explicit native browser observation and connection checks.
use anyhow::{Context, Result, ensure};
use clap::Subcommand;
use oe_browser::{BackendKind, BrowserTask, Observation, backend, source};
use oe_core::{SkillRoot, read_regular_bytes, sha256, write_new};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

const INPUT_LIMIT: usize = 8 * 1024 * 1024;
pub(crate) const BACKEND_SCHEMA: &str = "ontology-engineering.native-browser-backend-config/v1";

#[derive(Subcommand)]
pub enum BrowserCommand {
    /// Check the compiled adapter and locked observer locally; never connect.
    Doctor,
    /// Start only the owned Peekaboo transport; Chrome is never launched or configured.
    TransportStart {
        #[arg(long)]
        backend_config: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Verify an existing Peekaboo connection using a real read-only page listing.
    ConnectionCheck {
        #[arg(long)]
        backend_config: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Observe one exact background page without model calls or page actions.
    Observe {
        #[arg(long)]
        task: PathBuf,
        #[arg(long)]
        backend_config: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Observe a specific existing Chrome window through background AX only.
    AxObserve {
        #[arg(long)]
        backend_config: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        window_id: Option<u64>,
    },
}

/// JSON inputs are rooted and captured by bytes before any external activity.
pub(crate) struct InputDocument {
    pub value: Value,
    pub reference: Value,
}

pub(crate) fn input(root: &SkillRoot, path: &Path) -> Result<InputDocument> {
    let path = root.expand(path)?;
    let relative = path
        .strip_prefix(root.path())
        .context("browser_input_outside_skill_root")?;
    let bytes = read_regular_bytes(&path, INPUT_LIMIT)?;
    let value = oe_core::json::strict_json(&bytes)?;
    Ok(InputDocument {
        value,
        reference: json!({"path":relative.to_str().context("browser_input_path_not_utf8")?,
            "sha256":sha256(&bytes),"size_bytes":bytes.len()}),
    })
}

pub(crate) fn config(
    root: &SkillRoot,
    document: &InputDocument,
    task: &BrowserTask,
    output: PathBuf,
) -> Result<backend::BackendConfig> {
    let value = &document.value;
    let object = value
        .as_object()
        .context("browser_backend_configuration_required")?;
    ensure!(
        object.keys().all(|key| [
            "schema",
            "backend",
            "harness_socket",
            "osascript",
            "peekaboo_socket",
            "cdp_endpoint",
            "window_id",
            "timeout_seconds",
            "response_limit"
        ]
        .contains(&key.as_str()))
            && value["schema"] == BACKEND_SCHEMA,
        "browser_backend_configuration_invalid"
    );
    let kind: BackendKind = serde_json::from_value(value["backend"].clone())
        .context("browser_backend_kind_required")?;
    ensure!(kind == task.backend, "browser_backend_task_mismatch");
    backend::ensure_current_policy(kind)?;
    let mut config = backend::BackendConfig::new(output);
    if let Some(timeout) = value.get("timeout_seconds") {
        let seconds = timeout
            .as_u64()
            .filter(|v| (1..=60).contains(v))
            .context("browser_transport_timeout_out_of_range")?;
        config.timeout = Duration::from_secs(seconds);
    }
    if let Some(limit) = value.get("response_limit") {
        let limit = limit
            .as_u64()
            .filter(|v| (1024..=32 * 1024 * 1024).contains(v))
            .context("browser_response_limit_out_of_range")?;
        config.response_limit = usize::try_from(limit)?;
    }
    match kind {
        BackendKind::PeekabooBackground => {
            ensure!(
                !object.contains_key("osascript") && !object.contains_key("harness_socket"),
                "unused_browser_backend_option"
            );
            if let Some(value) = value.get("peekaboo_socket") {
                let socket = PathBuf::from(
                    value
                        .as_str()
                        .filter(|v| !v.is_empty())
                        .context("existing_peekaboo_socket_required")?,
                );
                ensure!(socket.is_absolute(), "browser_socket_must_be_absolute");
                oe_core::no_symlinks(socket.parent().context("peekaboo_socket_parent_required")?)?;
                config.peekaboo_socket = Some(socket);
            }
            if let Some(endpoint) = value.get("cdp_endpoint") {
                config.cdp_endpoint = Some(
                    endpoint
                        .as_str()
                        .filter(|v| !v.is_empty())
                        .context("existing_cdp_endpoint_invalid")?
                        .to_owned(),
                );
                backend::parse_existing_cdp_endpoint(config.cdp_endpoint.as_deref().unwrap())?;
            }
            if let Some(id) = value.get("window_id") {
                config.window_id = Some(
                    id.as_u64()
                        .filter(|v| *v > 0)
                        .context("peekaboo_exact_window_identity_required")?,
                );
            }
        }
        BackendKind::BrowserHarness => {
            ensure!(
                !object.contains_key("osascript"),
                "unused_browser_backend_option"
            );
            let socket = PathBuf::from(
                value["harness_socket"]
                    .as_str()
                    .filter(|v| !v.is_empty())
                    .context("existing_harness_socket_required")?,
            );
            ensure!(socket.is_absolute(), "browser_socket_must_be_absolute");
            oe_core::no_symlinks(&socket)?;
            config.harness_socket = Some(socket);
        }
        BackendKind::ChromeAppleEvents => {
            ensure!(
                !object.contains_key("harness_socket"),
                "unused_browser_backend_option"
            );
            if let Some(executable) = value.get("osascript") {
                let executable = PathBuf::from(
                    executable
                        .as_str()
                        .filter(|v| !v.is_empty())
                        .context("browser_osascript_required")?,
                );
                ensure!(
                    executable.is_absolute(),
                    "browser_osascript_must_be_absolute"
                );
                oe_core::no_symlinks(&executable)?;
                config.osascript = executable;
            }
        }
    }
    root.private_path(&config.output)?;
    Ok(config)
}

fn envelope(command: &str, result: Value, code: u8) -> (Value, u8) {
    (
        json!({"schema":"ontology-engineering.native-browser-command/v1", "command":command,
        "status":result["status"], "result":result,
        "semantic_execution":"not_run", "engineering_acceptance":"not_evaluated",
        "publication":"not_performed", "fact_admitted":false}),
        code,
    )
}

pub fn run(root: &SkillRoot, command: BrowserCommand) -> Result<(Value, u8)> {
    let starts_transport = matches!(&command, BrowserCommand::TransportStart { .. });
    match command {
        BrowserCommand::AxObserve {
            backend_config,
            output,
            window_id,
        } => {
            let document = input(root, &backend_config)?;
            let task = BrowserTask::parse(
                json!({"schema":oe_browser::task::TASK_SCHEMA,"goal":"Read-only existing Chrome window","url":"https://www.misumi.com.cn/"}),
            )?;
            let config = config(root, &document, &task, output)?;
            let result = backend::ax_observe(root, &config, window_id.or(config.window_id))?;
            Ok(envelope("ax-observe", result, 0))
        }
        BrowserCommand::TransportStart {
            backend_config,
            output,
        }
        | BrowserCommand::ConnectionCheck {
            backend_config,
            output,
        } => {
            let start = starts_transport;
            let document = input(root, &backend_config)?;
            let task = BrowserTask::parse(
                json!({"schema":oe_browser::task::TASK_SCHEMA,"goal":"Read-only connection check","url":"https://www.misumi.com.cn/"}),
            )?;
            let config = config(root, &document, &task, output)?;
            let result = if start {
                backend::start_worker(root, &config)?
            } else {
                backend::connection_check(root, &config)?
            };
            Ok(envelope(
                if start {
                    "transport-start"
                } else {
                    "connection-check"
                },
                result,
                0,
            ))
        }
        BrowserCommand::Observe {
            task,
            backend_config,
            output,
        } => {
            let task_document = input(root, &task)?;
            let task = BrowserTask::parse(task_document.value.clone())?;
            backend::ensure_current_policy(task.backend)?;
            let config_document = input(root, &backend_config)?;
            let output = root.private_path(&output)?;
            let config = config(root, &config_document, &task, output.join("backend"))?;
            let source = source::identity(root)?;
            let snapshot = source::snapshot(root)?;
            root.new_private_dir(&output)?;
            write_new(
                &output.join("inputs.json"),
                &json!({"task":task_document.reference,"backend_config":config_document.reference,"source":source}),
            )?;
            if config.cdp_endpoint.is_none() {
                let result = backend::ax_observe(root, &config, config.window_id)?;
                source::ensure_current(root, &source)?;
                write_new(&output.join("result.json"), &result)?;
                return Ok(envelope("observe", result, 0));
            }
            let mut browser = backend::connect(root, &task, &snapshot, &config)?;
            let outcome = browser.observe(false);
            let cleanup = browser.close().unwrap_or_else(
                |_| json!({"status":"cleanup_unconfirmed","target_retained_for_diagnosis":true}),
            );
            let observation = outcome?;
            source::ensure_current(root, &source)?;
            write_new(
                &output.join("observation.json"),
                &serde_json::to_value(&observation)?,
            )?;
            let result = json!({"status":"observed","session":observation.session,"tab":cleanup,"model_calls":0,"browser_actions":0,"semantic_execution":"not_run","engineering_acceptance":"not_evaluated"});
            write_new(&output.join("result.json"), &result)?;
            Ok(envelope("observe", result, 0))
        }
        BrowserCommand::Doctor => Ok(envelope("doctor", source::doctor(root)?, 0)),
    }
}
