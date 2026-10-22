use anyhow::{Result, ensure};
use clap::Subcommand;
use oe_core::{SkillRoot, read_regular_bytes, sha256};
use serde_json::{Value, json};
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum DesignAttemptCommand {
    /// Freeze complete current work, source preparation and independent design intent.
    Prepare {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Re-read the current source closure and every frozen input.
    Verify {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        prepared: PathBuf,
    },
    /// Execute current work and check existing artifacts; does not execute native CAD.
    Record {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[command(flatten)]
        tools: crate::KnowledgeTools,
    },
    /// Recheck an explicit reference-only /2 CAD association and its original sources.
    VerifyRecord {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        recorded: PathBuf,
    },
}
pub fn run(root: &SkillRoot, command: DesignAttemptCommand) -> Result<Value> {
    let binding = crate::v2::require_execution(root, "design_attempt")?;
    let input = match &command {
        DesignAttemptCommand::Prepare { input, .. }
        | DesignAttemptCommand::Verify { input, .. }
        | DesignAttemptCommand::Record { input, .. }
        | DesignAttemptCommand::VerifyRecord { input, .. } => input,
    };
    let path = root.data_expand(input)?;
    ensure!(
        path.starts_with(root.data_path()),
        "design_attempt_input_outside_root"
    );
    let raw = read_regular_bytes(&path, 16 * 1024 * 1024)?;
    let reference = oe_contracts::ArtifactReference {
        path: root.portable_data_path(&path)?,
        sha256: sha256(&raw),
    };
    let completion = match &command {
        DesignAttemptCommand::Prepare { output, .. } => Some((output.clone(), "prepare")),
        DesignAttemptCommand::Record { output, .. } => Some((output.clone(), "record")),
        DesignAttemptCommand::Verify { .. } => None,
        DesignAttemptCommand::VerifyRecord { .. } => None,
    };
    let mut result = match command {
        DesignAttemptCommand::Prepare { output, .. } => {
            oe_engineering::design_attempt::prepare_to(root, &reference, &output)?
        }
        DesignAttemptCommand::Verify { prepared, .. } => {
            let request = serde_json::from_value(oe_core::json::strict_json(&raw)?)?;
            oe_engineering::design_attempt::verify_preparation(root, &request, &prepared)?
        }
        DesignAttemptCommand::Record { output, tools, .. } => {
            let value = oe_core::json::strict_json(&raw)?;
            if value["schema"] == "oev2.design-attempt-record/2" {
                let request = serde_json::from_value(value)?;
                return record_cad_binding(
                    root, &request, &output, &path, &raw, &reference, &binding,
                );
            } else {
                let request = serde_json::from_value(value)?;
                oe_engineering::design_attempt::record(root, &request, &output, &tools.resolve()?)?
            }
        }
        DesignAttemptCommand::VerifyRecord { recorded, .. } => {
            let request = serde_json::from_value(oe_core::json::strict_json(&raw)?)?;
            oe_engineering::design_attempt::cad_binding::verify_record(root, &request, &recorded)?
        }
    };
    ensure!(
        read_regular_bytes(&path, 16 * 1024 * 1024)? == raw,
        "design_attempt_cli_request_changed"
    );
    if let Some((output, operation)) = completion {
        let dir = root.private_path(&output)?;
        let core = read_regular_bytes(&dir.join("completed.json"), 16 * 1024 * 1024)?;
        oe_core::write_new_bytes(&dir.join("cli-input.json"), &raw)?;
        oe_core::write_new(&dir.join("execution-binding.json"), &binding)?;
        let identity = read_regular_bytes(&dir.join("execution-binding.json"), 16 * 1024 * 1024)?;
        // The library inventory is immutable. This separate outer terminal binds
        // its bytes and the actual CLI identity without an identity/hash cycle.
        let final_binding = crate::v2::require_execution(root, "design_attempt_completion")?;
        for key in [
            "normalizer",
            "cli",
            "semantic_contracts_sha256",
            "executable_sha256",
        ] {
            ensure!(
                final_binding[key] == binding[key],
                "design_attempt_execution_identity_changed:{key}"
            );
        }
        oe_core::write_new(&dir.join("execution-recheck.json"), &final_binding)?;
        let recheck = read_regular_bytes(&dir.join("execution-recheck.json"), 16 * 1024 * 1024)?;
        ensure!(
            read_regular_bytes(&path, 16 * 1024 * 1024)? == raw
                && read_regular_bytes(&dir.join("completed.json"), 16 * 1024 * 1024)? == core,
            "design_attempt_cli_completion_changed"
        );
        let terminal = json!({"schema":"oev2.design-attempt-cli-completed/1","status":"completed","operation":operation,"input":reference,
            "core_completed":{"path":"completed.json","sha256":sha256(&core)},
            "input_copy":{"path":"cli-input.json","sha256":sha256(&raw)},
            "execution_binding":{"path":"execution-binding.json","sha256":sha256(&identity)},
            "execution_recheck":{"path":"execution-recheck.json","sha256":sha256(&recheck)},
            "design_complete":false,"adopted":false,"project_mutated":false});
        oe_core::write_new(&dir.join("cli-completed.json"), &terminal)?;
        result["cli_completion"] = terminal;
    }
    result["execution_binding"] = binding;
    Ok(result)
}

// /2 preflights the same exact core plan plus all four existing CLI attachment
// files. The legacy /1 publication path above retains its original meaning.
fn record_cad_binding(
    root: &SkillRoot,
    request: &oe_contracts::EngineeringDesignAttemptRecordRequest2,
    output: &std::path::Path,
    input_path: &std::path::Path,
    raw: &[u8],
    reference: &oe_contracts::ArtifactReference,
    binding: &Value,
) -> Result<Value> {
    use oe_engineering::design_attempt::cad_binding::{bounded_json, prepare_record};
    let plan = prepare_record(root, request)?;
    let recheck = crate::v2::require_execution(root, "design_attempt_completion")?;
    let same_identity = |current: &Value| -> Result<()> {
        for key in [
            "normalizer",
            "cli",
            "semantic_contracts_sha256",
            "executable_sha256",
        ] {
            ensure!(
                current[key] == binding[key],
                "design_attempt_execution_identity_changed:{key}"
            );
        }
        ensure!(
            read_regular_bytes(input_path, 16 * 1024 * 1024)? == raw,
            "design_attempt_cli_request_changed"
        );
        Ok(())
    };
    same_identity(&recheck)?;
    let start_bytes = bounded_json(binding)?;
    let recheck_bytes = bounded_json(&recheck)?;
    let terminal = json!({"schema":"oev2.design-attempt-cli-completed/1","status":"completed","operation":"record","input":reference,
        "core_completed":{"path":"completed.json","sha256":plan.core_completed_sha256()},
        "input_copy":{"path":"cli-input.json","sha256":sha256(raw)},
        "execution_binding":{"path":"execution-binding.json","sha256":sha256(&start_bytes)},
        "execution_recheck":{"path":"execution-recheck.json","sha256":sha256(&recheck_bytes)},
        "design_complete":false,"adopted":false,"project_mutated":false});
    let outer = std::collections::BTreeMap::from([
        ("cli-input.json".into(), raw.to_vec()),
        ("execution-binding.json".into(), start_bytes),
        ("execution-recheck.json".into(), recheck_bytes),
        ("cli-completed.json".into(), bounded_json(&terminal)?),
    ]);
    let mut result = plan.publish(root, output, outer, || {
        let current = crate::v2::require_execution(root, "design_attempt_terminal")?;
        same_identity(&current)
    })?;
    result["cli_completion"] = terminal;
    result["execution_binding"] = binding.clone();
    Ok(result)
}
