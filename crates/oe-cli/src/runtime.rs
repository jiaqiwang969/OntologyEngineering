//! Runtime operations use native Rust orchestration and bounded external tools.
use anyhow::Result;
use clap::{Args, Subcommand};
use oe_core::SkillRoot;
use oe_semantic::{runtime_build, runtime_install};
use std::path::{Path, PathBuf};

fn tool_path(root: &SkillRoot, path: &Path) -> Result<PathBuf> {
    if path.components().count() > 1 {
        root.expand(path)
    } else {
        Ok(path.to_owned()) // bare executable names retain normal PATH lookup
    }
}

#[derive(Args)]
pub struct SourceArgs {
    /// Explicit Semantica source checkout, read-only even during build.
    #[arg(long)]
    source_root: PathBuf,
    /// New evidence/build directory below this skill's var/.
    #[arg(long)]
    output: PathBuf,
    #[arg(long, default_value = "python3")]
    python: PathBuf,
    #[arg(long, default_value = "git")]
    git: PathBuf,
    #[arg(long, default_value = "npm")]
    npm: PathBuf,
    #[arg(long, default_value = "node")]
    node: PathBuf,
    /// Python build dependencies only; npm has its own cache policy.
    #[arg(long)]
    wheelhouse: Option<PathBuf>,
    #[arg(long)]
    npm_offline: bool,
    /// Current Rust dispatch input; historical source qualification remains separate.
    #[arg(long)]
    native_input: Option<PathBuf>,
}
impl SourceArgs {
    fn options(self, root: &SkillRoot) -> Result<runtime_build::BuildOptions> {
        let mut options =
            runtime_build::BuildOptions::new(root.expand(&self.source_root)?, self.output);
        options.python = tool_path(root, &self.python)?;
        options.git = tool_path(root, &self.git)?;
        options.npm = tool_path(root, &self.npm)?;
        options.node = tool_path(root, &self.node)?;
        options.wheelhouse = self.wheelhouse.map(|p| root.expand(&p)).transpose()?;
        options.npm_offline = self.npm_offline;
        Ok(options)
    }
}

#[derive(Subcommand)]
pub enum RuntimeCommand {
    /// Retired v1 Python entry: arguments are readable, execution is rejected.
    Preflight {
        #[arg(long, default_value = "python3")]
        python: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Retired v1 installer parameters; never creates or executes a Python runtime.
    Install {
        #[arg(long, default_value = "python3")]
        python: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        wheelhouse: Option<PathBuf>,
        #[arg(long)]
        offline: bool,
    },
    /// Observe full Git source and locked build inputs; no build or installation.
    InspectSource(SourceArgs),
    /// Qualify source, then use an explicit current Rust builder input; never builds Python wheels.
    BuildSource(SourceArgs),
    /// Dispatch through the same source-bound Rust runtime/version owner.
    Native {
        #[arg(long)]
        input: PathBuf,
    },
    /// Explicit /2 independent Rust reproduction, preserving complete debug and signature bytes.
    Reproduce {
        #[arg(long)]
        input: PathBuf,
    },
}

pub fn execute(root: &SkillRoot, command: RuntimeCommand) -> Result<u8> {
    let response = match command {
        RuntimeCommand::Reproduce { input } => {
            let path = root.private_path(&input)?;
            let request: oe_contracts::CodeRustRuntimeDispatchRequest = serde_json::from_value(
                oe_core::json::strict_json(&oe_core::read_regular_bytes(&path, 16 * 1024 * 1024)?)?,
            )?;
            let response = oe_evolution::runtime_identity::dispatch_reproduction2(root, &request)?;
            let code = if response.execution == "completed" {
                0
            } else {
                1
            };
            println!("{}", serde_json::to_string_pretty(&response)?);
            return Ok(code);
        }
        RuntimeCommand::Native { input } => {
            return crate::v2::maintain(root, crate::v2::MaintainCommand::RuntimeNative { input });
        }
        RuntimeCommand::Preflight { python, output } => {
            runtime_install::preflight(root, &tool_path(root, &python)?, &output)?
        }
        RuntimeCommand::Install {
            python,
            output,
            wheelhouse,
            offline,
        } => {
            let mut options =
                runtime_install::InstallOptions::new(tool_path(root, &python)?, output);
            options.wheelhouse = wheelhouse.map(|p| root.expand(&p)).transpose()?;
            options.offline = offline;
            runtime_install::install(root, &options)?
        }
        RuntimeCommand::InspectSource(args) => {
            runtime_build::inspect_source(root, &args.options(root)?)?
        }
        RuntimeCommand::BuildSource(args) => {
            if let Some(input) = args.native_input.clone() {
                anyhow::ensure!(
                    root.expand(&args.source_root)?.canonicalize()? == root.path(),
                    "rust_runtime_dispatch_source_root"
                );
                return crate::v2::maintain(
                    root,
                    crate::v2::MaintainCommand::RuntimeNative { input },
                );
            }
            // Read-only original source checks are still useful and retain their
            // complete original report. They never become current compilation.
            let options = args.options(root)?;
            let mut report = runtime_build::inspect_source(root, &options)?;
            report["action"] = serde_json::json!("build");
            if report["status"] == "ready" {
                let observed = oe_core::process::run_command(
                    &oe_core::process::CommandSpec {
                        executable: options.node,
                        arguments: vec!["--version".into()],
                        cwd: root.path().to_owned(),
                        environment: std::collections::BTreeMap::new(),
                        remove_environment: vec![],
                        timeout: std::time::Duration::from_secs(5),
                        output_limit: 65536,
                    },
                    &root
                        .private_path(&options.output_dir)?
                        .join("current-node-preflight"),
                );
                report["status"] = serde_json::json!("blocked");
                report["error"] = serde_json::json!(match observed {
                    Ok(p) if p.status == "completed" && p.exit_code == Some(0) =>
                        "current_rust_build_input_required",
                    _ => "current_frontend_tool_not_available",
                });
            }
            report["current_runtime_language"] = serde_json::json!("rust");
            report["historical_wheel_build_dispatched"] = serde_json::json!(false);
            oe_core::write_new(
                &root
                    .private_path(&options.output_dir)?
                    .join("current-build-preflight-report.json"),
                &report,
            )?;
            report
        }
    };
    let passed = response.get("passed").is_none_or(|v| v == true)
        && response.get("ok").is_none_or(|v| v == true)
        && matches!(
            response["status"].as_str(),
            Some("completed" | "passed" | "ready" | "installed" | "verified")
        );
    println!("{}", serde_json::to_string_pretty(&response)?);
    Ok(if passed { 0 } else { 1 })
}
