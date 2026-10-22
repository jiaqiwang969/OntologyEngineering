use anyhow::{Result, ensure};
use clap::Subcommand;
use oe_core::{SkillRoot, read_regular_bytes, write_new, write_new_bytes};
use serde_json::{Value, json};
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum IncadCommand {
    /// Prepare the original specification alongside an existing current Project work/3.
    Prepare {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Re-read the same Project/source bytes and compare a prior preparation.
    Verify {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        prepared: PathBuf,
    },
    /// Verify selected original CAD ZIPs, members and current Project, then stage copies.
    StageCad {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Re-read the full selected source and staged payload closure without launching CAD.
    VerifyCad {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        staged: PathBuf,
    },
    /// Bind selected original STEP/current work to an existing local NX job, without dispatch.
    PrepareNxAssembly {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Recheck current source/work/worker and complete original prepared NX inputs.
    VerifyNxAssembly {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        prepared: PathBuf,
    },
}
fn read(root: &SkillRoot, p: &std::path::Path) -> Result<Vec<u8>> {
    let p = root.expand(p)?;
    ensure!(p.starts_with(root.path()), "incad_cli_input_outside_root");
    read_regular_bytes(&p, 16 * 1024 * 1024)
}
pub fn run(root: &SkillRoot, command: IncadCommand) -> Result<Value> {
    if matches!(
        &command,
        IncadCommand::PrepareNxAssembly { .. } | IncadCommand::VerifyNxAssembly { .. }
    ) {
        return run_nx_assembly(root, command);
    }
    if matches!(
        &command,
        IncadCommand::StageCad { .. } | IncadCommand::VerifyCad { .. }
    ) {
        return run_cad(root, command);
    }
    let binding = crate::v2::require_execution(root, "incad_source_preparation")?;
    let (input, output, verify) = match command {
        IncadCommand::Prepare { input, output } => (input, output, false),
        IncadCommand::Verify { input, prepared } => (input, prepared, true),
        _ => unreachable!("CAD selection routed above"),
    };
    let raw = read(root, &input)?;
    let request = serde_json::from_value(oe_core::json::strict_json(&raw)?)?;
    let prepared = oe_engineering::incad::prepare(root, &request)?;
    let result = if verify {
        let terminal = oe_core::json::strict_json(&read(root, &output.join("completed.json"))?)?;
        ensure!(
            terminal["status"] == "prepared"
                && terminal["work_sha256"] == oe_core::sha256(&prepared.work_bytes),
            "incad_preparation_not_completed"
        );
        let previous = oe_core::json::strict_json(&read(root, &output.join("preparation.json"))?)?;
        ensure!(
            read(root, &output.join("request.json"))? == raw
                && read(root, &output.join("work.json"))? == prepared.work_bytes
                && read(root, &output.join("specifications.json"))?
                    == prepared.intake.specifications_bytes()
                && read(root, &output.join("zero-design-task.json"))?
                    == prepared.intake.task_bytes(),
            "incad_preparation_artifact_changed"
        );
        oe_engineering::incad::verify_preparation(root, &request, &previous)?
    } else {
        let dir = root.new_private_dir(&output)?;
        write_new_bytes(&dir.join("request.json"), &raw)?;
        write_new_bytes(&dir.join("work.json"), &prepared.work_bytes)?;
        write_new_bytes(
            &dir.join("specifications.json"),
            prepared.intake.specifications_bytes(),
        )?;
        write_new_bytes(
            &dir.join("zero-design-task.json"),
            prepared.intake.task_bytes(),
        )?;
        write_new(&dir.join("preparation.json"), &prepared.result)?;
        prepared.ensure_current(root)?;
        ensure!(read(root, &input)? == raw, "incad_request_changed");
        let receipt = json!({"status":"prepared","output":dir,"execution_binding":binding,
            "engineering_applicability":"not_reviewed","task_work_goal_relation":"not_reviewed",
            "work_sha256":oe_core::sha256(&prepared.work_bytes),"native_execution":"not_run","engineering_adoption":"not_assessed"});
        write_new(&dir.join("completed.json"), &receipt)?;
        receipt
    };
    ensure!(read(root, &input)? == raw, "incad_request_changed");
    Ok(result)
}

fn run_nx_assembly(root: &SkillRoot, command: IncadCommand) -> Result<Value> {
    let binding = crate::v2::require_execution(root, "incad_nx_assembly_bridge")?;
    let (input, output, verify) = match command {
        IncadCommand::PrepareNxAssembly { input, output } => (input, output, false),
        IncadCommand::VerifyNxAssembly { input, prepared } => (input, prepared, true),
        _ => unreachable!(),
    };
    let raw = read(root, &input)?;
    let request: oe_contracts::IncadNxAssemblyBridgeRequest =
        serde_json::from_value(oe_core::json::strict_json(&raw)?)?;
    let result = if verify {
        oe_engineering::incad::cad::nx_assembly::verify(root, &request, &output)?
    } else {
        oe_engineering::incad::cad::nx_assembly::prepare(root, &request, &output)?
    };
    ensure!(
        read(root, &input)? == raw,
        "incad_nx_bridge_cli_request_changed"
    );
    let end = crate::v2::require_execution(root, "incad_nx_assembly_bridge_completion")?;
    for name in [
        "normalizer",
        "cli",
        "semantic_contracts_sha256",
        "executable_sha256",
    ] {
        ensure!(
            binding[name] == end[name],
            "incad_nx_bridge_cli_identity_changed"
        );
    }
    Ok(
        json!({"schema":"oev2.incad-nx-assembly-cli-result/1","bridge":result,
        "input_sha256":oe_core::sha256(&raw),"execution_binding":binding,"execution_recheck":end,
        "native_execution":"not_run","engineering_adoption":"not_assessed"}),
    )
}

fn run_cad(root: &SkillRoot, command: IncadCommand) -> Result<Value> {
    let binding = crate::v2::require_execution(root, "incad_cad_selection")?;
    let (input, output, verify) = match command {
        IncadCommand::StageCad { input, output } => (input, output, false),
        IncadCommand::VerifyCad { input, staged } => (input, staged, true),
        _ => unreachable!(),
    };
    let raw = read(root, &input)?;
    let request = serde_json::from_value(oe_core::json::strict_json(&raw)?)?;
    let mut result = if verify {
        oe_engineering::incad::cad::verify_staged(root, &request, &output)?
    } else {
        oe_engineering::incad::cad::stage(root, &request, &output)?
    };
    ensure!(read(root, &input)? == raw, "incad_cad_cli_input_changed");
    let final_binding = crate::v2::require_execution(root, "incad_cad_selection_completion")?;
    for key in [
        "normalizer",
        "cli",
        "semantic_contracts_sha256",
        "executable_sha256",
    ] {
        ensure!(
            final_binding[key] == binding[key],
            "incad_cad_execution_identity_changed:{key}"
        );
    }
    if !verify {
        let dir = root.private_path(&output)?;
        let core = read(root, &output.join("completed.json"))?;
        oe_core::write_new_bytes(&dir.join("cli-input.json"), &raw)?;
        oe_core::write_new(&dir.join("execution-binding.json"), &binding)?;
        oe_core::write_new(&dir.join("execution-recheck.json"), &final_binding)?;
        let terminal = json!({"schema":"oev2.incad-cad-cli-completed/1","status":"completed",
            "input_sha256":oe_core::sha256(&raw),"core_completed_sha256":oe_core::sha256(&core),
            "execution_binding_sha256":oe_core::sha256(read(root,&output.join("execution-binding.json"))?),
            "execution_recheck_sha256":oe_core::sha256(read(root,&output.join("execution-recheck.json"))?),
            "native_execution":"not_run","adopted":false});
        ensure!(
            read(root, &input)? == raw && read(root, &output.join("completed.json"))? == core,
            "incad_cad_cli_completion_changed"
        );
        oe_core::write_new(&dir.join("cli-completed.json"), &terminal)?;
        result["cli_completion"] = terminal;
    } else {
        verify_cli_completion(
            &oe_core::json::strict_json(&read(root, &output.join("cli-completed.json"))?)?,
            &raw,
            &read(root, &output.join("completed.json"))?,
            &read(root, &output.join("cli-input.json"))?,
            &read(root, &output.join("execution-binding.json"))?,
            &read(root, &output.join("execution-recheck.json"))?,
            &binding,
            &final_binding,
        )?;
    }
    ensure!(
        read(root, &input)? == raw,
        "incad_cad_cli_input_changed_after_completion"
    );
    result["execution_binding"] = binding;
    Ok(result)
}

fn verify_cli_completion(
    terminal: &Value,
    input: &[u8],
    core: &[u8],
    input_copy: &[u8],
    start_raw: &[u8],
    end_raw: &[u8],
    actual_start: &Value,
    actual_end: &Value,
) -> Result<()> {
    let expected = json!({"schema":"oev2.incad-cad-cli-completed/1","status":"completed",
        "input_sha256":oe_core::sha256(input),"core_completed_sha256":oe_core::sha256(core),
        "execution_binding_sha256":oe_core::sha256(start_raw),"execution_recheck_sha256":oe_core::sha256(end_raw),
        "native_execution":"not_run","adopted":false});
    ensure!(
        *terminal == expected && input_copy == input,
        "incad_cad_cli_terminal_changed"
    );
    let start = oe_core::json::strict_json(start_raw)?;
    let end = oe_core::json::strict_json(end_raw)?;
    for key in [
        "normalizer",
        "cli",
        "semantic_contracts_sha256",
        "executable_sha256",
    ] {
        let expected_start = actual_start
            .get(key)
            .ok_or_else(|| anyhow::anyhow!("incad_cad_current_execution_key_missing:{key}"))?;
        let expected_end = actual_end
            .get(key)
            .ok_or_else(|| anyhow::anyhow!("incad_cad_current_execution_key_missing:{key}"))?;
        ensure!(
            start.get(key) == Some(expected_start)
                && end.get(key) == Some(expected_end)
                && expected_start == expected_end,
            "incad_cad_recorded_execution_identity_changed:{key}"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn identity() -> Value {
        json!({"normalizer":{"status":"matched","source":"current"},"cli":{"status":"matched","source":"current"},"semantic_contracts_sha256":"actual-contracts","executable_sha256":"actual-program"})
    }
    fn terminal(start: &[u8], end: &[u8]) -> Value {
        json!({"schema":"oev2.incad-cad-cli-completed/1","status":"completed",
        "input_sha256":oe_core::sha256(b"input"),"core_completed_sha256":oe_core::sha256(b"core"),
        "execution_binding_sha256":oe_core::sha256(start),"execution_recheck_sha256":oe_core::sha256(end),
        "native_execution":"not_run","adopted":false})
    }
    #[test]
    fn incad_cad_cli_authority_and_rebound_identity_are_refused() {
        let current = identity();
        let raw = serde_json::to_vec(&current).unwrap();
        let good = terminal(&raw, &raw);
        verify_cli_completion(
            &good, b"input", b"core", b"input", &raw, &raw, &current, &current,
        )
        .unwrap();
        for key in ["native_execution", "adopted", "extra_authority"] {
            let mut changed = good.clone();
            changed[key] = if key == "native_execution" {
                json!("completed")
            } else {
                json!(true)
            };
            assert!(
                verify_cli_completion(
                    &changed, b"input", b"core", b"input", &raw, &raw, &current, &current
                )
                .is_err(),
                "{key}"
            );
        }
        for key in [
            "normalizer",
            "cli",
            "semantic_contracts_sha256",
            "executable_sha256",
        ] {
            let mut authored = current.clone();
            authored[key] = json!("author-rebound");
            let fake = serde_json::to_vec(&authored).unwrap();
            assert!(
                verify_cli_completion(
                    &terminal(&fake, &fake),
                    b"input",
                    b"core",
                    b"input",
                    &fake,
                    &fake,
                    &current,
                    &current
                )
                .is_err(),
                "{key}"
            );
        }
        assert!(
            verify_cli_completion(
                &good,
                b"input",
                b"core",
                b"different-input",
                &raw,
                &raw,
                &current,
                &current
            )
            .is_err()
        );
    }
}
