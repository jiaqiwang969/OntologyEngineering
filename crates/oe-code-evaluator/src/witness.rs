//! Independent execution boundary. A caller supplies a frozen experiment and
//! budget, never its own observations or passed claims. No semantic/kernel links.
use super::{compare, digest, invocation, require_fixed_evaluator};
use anyhow::{Context, Result, ensure};
use oe_contracts::{
    ArtifactReference, CodeBehaviorWitness, CodeComparison, CodeComparisonRequest,
    CodeScopeWitnessDispatch,
};
use oe_core::{
    SkillRoot, fingerprint_file,
    process::{CommandSpec, run_command},
    sha256, write_new,
};
use oe_wire::json::strict_json;
use std::{collections::BTreeMap, path::Path, time::Duration};

const ORACLE: &str = "07c0481d530ff868ca777ae49dc19803bcb0523347baf6b06b7e5ca243be30fe";
const PROTOCOL: &str = "12cf2e3bcd171ac1000373257553dc0202d9bc42a7d8ff46f910338158ea2809";
const SCOPE: &str = "b1470d9980755811c815be24cdc041740de5f55e28232c327f0845d160455226";

// This independent evaluator embeds only fixed expected data, never the
// candidate runtime. The original /1 oracle and six-case branch stay exact.
const Q40_ORACLE: &str = "fb556d112c0302389240f83b536c82aa7e5079a36be7c2cec2645a9d8a5b51ce";
const Q40_CATALOG: &str = "31a5ac5f7b95b976c75d809293f613fbd0fb946d0a330fd48d4064550981bbc4";
fn fixed_profile(request: &CodeScopeWitnessDispatch) -> Result<(&'static str, usize)> {
    match request.schema.as_str() {
        "oev2.code-scope-witness-dispatch/1" => Ok((ORACLE, 6)),
        "oev2.code-scope-witness-dispatch/2" => {
            let raw = include_bytes!("../../../runtime/q40-replay-cases.json");
            let catalog = include_bytes!("../../../runtime/q40-catalog.json");
            ensure!(
                sha256(raw) == Q40_ORACLE && sha256(catalog) == Q40_CATALOG,
                "witness_q40_fixed_catalog_changed"
            );
            let catalog = strict_json(catalog)?;
            ensure!(
                catalog["schema"] == "oev2.code-replay-catalog/2"
                    && catalog["oracle_sha256"] == Q40_ORACLE
                    && catalog["conditions_sha256"] == request.experiment.conditions_sha256
                    && strict_json(raw)? == serde_json::to_value(&request.experiment.cases)?,
                "witness_q40_fixed_cases_changed"
            );
            Ok((Q40_ORACLE, 40))
        }
        _ => anyhow::bail!("witness_dispatch_schema"),
    }
}

pub fn run(request: CodeScopeWitnessDispatch) -> Result<CodeBehaviorWitness> {
    request
        .validate()
        .map_err(|e| anyhow::anyhow!("witness_dispatch_contract:{e:?}"))?;
    let (oracle, denominator) = fixed_profile(&request)?;
    let e = &request.experiment;
    require_fixed_evaluator(e)?;
    ensure!(
        digest(&serde_json::to_value(e)?)? == request.experiment_sha256
            && e.oracle_sha256 == oracle
            && e.protocol_expectations_sha256 == PROTOCOL
            && e.scope_sha256 == SCOPE
            && e.cases.len() == denominator
            && e.protocol_case_ids.len() == 19
            && e.driver.source_root == e.evaluator.source_root,
        "witness_fixed_authorized_closure_changed"
    );
    ensure!(
        (1..=600_000).contains(&request.dispatch_budget.timeout_ms)
            && (1024..=4 * 1024 * 1024).contains(&request.dispatch_budget.output_bytes),
        "witness_dispatch_budget"
    );
    let driver = fingerprint_file(Path::new(&e.driver.path), 512 * 1024 * 1024)?;
    ensure!(
        driver.sha256 == e.driver.sha256 && driver.bytes == e.driver.bytes,
        "witness_real_driver_changed"
    );
    let root = SkillRoot::new(&e.driver.source_root)?;
    let folder = root.new_private_dir(Path::new(&request.evidence_path))?;
    let experiment_path = folder.join("experiment.json");
    write_new(&experiment_path, &serde_json::to_value(e)?)?;
    let frozen_experiment_bytes = oe_core::read_regular_bytes(&experiment_path, 16 * 1024 * 1024)?;
    let replay_name = format!(
        "scope-case-replay-{}-{}",
        std::process::id(),
        &sha256(request.evidence_path.as_bytes())[..16]
    );
    let replay_path = format!("{}/{replay_name}", request.evidence_path);
    let process = run_command(
        &CommandSpec {
            executable: e.driver.path.clone().into(),
            arguments: vec![
                "run".into(),
                e.driver.source_root.clone().into(),
                experiment_path.clone().into_os_string(),
                request.experiment_sha256.clone().into(),
                replay_path.clone().into(),
            ],
            cwd: e.driver.source_root.clone().into(),
            environment: BTreeMap::new(),
            remove_environment: vec![
                "RUSTFLAGS".into(),
                "CARGO_ENCODED_RUSTFLAGS".into(),
                "RUSTC_WRAPPER".into(),
                "RUSTC_WORKSPACE_WRAPPER".into(),
            ],
            timeout: Duration::from_millis(request.dispatch_budget.timeout_ms),
            output_limit: usize::try_from(request.dispatch_budget.output_bytes)?,
        },
        &folder.join("driver-dispatch"),
    )?;
    require_fixed_evaluator(e)?;
    let after = fingerprint_file(Path::new(&e.driver.path), 512 * 1024 * 1024)?;
    ensure!(
        after == driver
            && oe_core::read_regular_bytes(&experiment_path, 16 * 1024 * 1024)?
                == frozen_experiment_bytes,
        "witness_inputs_changed_during_dispatch"
    );
    let complete = process.status == "completed" && process.exit_code == Some(0);
    let mut all_actual_processes_complete = complete;
    let comparison: Option<CodeComparison> = if complete {
        let actual: CodeComparison = serde_json::from_value(strict_json(&process.stdout)?)?;
        actual
            .validate()
            .map_err(|x| anyhow::anyhow!("witness_comparison_contract:{x:?}"))?;
        ensure!(
            actual.experiment_id == e.experiment_id
                && actual.driver_sha256 == e.driver.sha256
                && actual.evaluator_sha256 == e.evaluator.sha256
                && actual.oracle_sha256 == oracle
                && actual.baseline_sha256 == e.baseline.manifest_sha256
                && actual.candidate_sha256 == e.candidate.manifest_sha256
                && actual.fixed_denominator == denominator as u64,
            "witness_actual_comparison_changed"
        );
        // These bytes came from a fresh actual process; reports pasted by a user
        // are never consulted in this boundary. Retain the complete raw chain.
        let saved = oe_core::read_regular_bytes(
            &root
                .private_path(Path::new(&replay_path))?
                .join("comparison.json"),
            4 * 1024 * 1024,
        )?;
        ensure!(
            strict_json(&saved)? == serde_json::to_value(&actual)?,
            "witness_driver_stdout_saved_comparison_changed"
        );
        let actual_input = oe_core::read_regular_bytes(
            &root
                .private_path(Path::new(&replay_path))?
                .join("comparison-input.json"),
            16 * 1024 * 1024,
        )?;
        let runs: CodeComparisonRequest = serde_json::from_value(strict_json(&actual_input)?)?;
        ensure!(
            runs.experiment == *e && runs.observations.len() == denominator * 2,
            "witness_actual_fixed_denominator_changed"
        );
        for run in &runs.observations {
            let artifact = match run.side.as_str() {
                "baseline" => &e.baseline,
                "candidate" => &e.candidate,
                _ => anyhow::bail!("witness_unknown_run_side"),
            };
            let case = e
                .cases
                .iter()
                .find(|c| c.case_id == run.case_id)
                .context("witness_unknown_run_case")?;
            let side_root = SkillRoot::new(&artifact.source_root)?;
            let relative = format!(
                "var/code-replays/{replay_name}/{}/{}/{}",
                run.side, run.case_id, run.run_id
            );
            let run_folder = side_root.private_path(Path::new(&relative))?;
            let invocation_file = run_folder.join("invocation.json");
            let input = oe_core::read_regular_bytes(&invocation_file, 1024 * 1024)?;
            ensure!(
                sha256(&input) == run.invocation_sha256
                    && strict_json(&input)? == serde_json::to_value(invocation(case, run.run_id))?,
                "witness_actual_invocation_changed"
            );
            let raw_process = oe_core::read_regular_bytes(
                &run_folder.join("process/process.json"),
                4 * 1024 * 1024,
            )?;
            ensure!(
                strict_json(&raw_process)? == run.process,
                "witness_actual_process_record_changed"
            );
            let stdout =
                oe_core::read_regular_bytes(&run_folder.join("process/stdout"), 16 * 1024 * 1024)?;
            let stderr =
                oe_core::read_regular_bytes(&run_folder.join("process/stderr"), 16 * 1024 * 1024)?;
            ensure!(
                run.process["stdout"]["sha256"] == sha256(&stdout)
                    && run.process["stderr"]["sha256"] == sha256(&stderr)
                    && run.stdout_bytes == Some(stdout.len() as u64)
                    && run.stderr_bytes == Some(stderr.len() as u64)
                    && run.process["executable"]["identity"]["sha256"] == artifact.runner.sha256
                    && run.process["executable"]["unchanged"] == true
                    && run.process["arguments"] == serde_json::json!(["run", invocation_file])
                    && run.process["cwd"] == artifact.source_root,
                "witness_actual_stream_or_runner_identity_changed"
            );
            if let Some(observed) = &run.observed {
                ensure!(
                    strict_json(&stdout)? == serde_json::to_value(observed)?,
                    "witness_actual_stdout_observation_changed"
                );
            }
            all_actual_processes_complete &= run.process["status"] == "completed"
                && run.process["exit_code"] == 0
                && run.process["capture_complete"] == true;
        }
        ensure!(
            compare(runs)? == actual,
            "witness_independent_actual_comparison_changed"
        );
        Some(actual)
    } else {
        None
    };
    let process_path = folder.join("driver-dispatch/process.json");
    let raw = oe_core::read_regular_bytes(&process_path, 4 * 1024 * 1024)?;
    let reference = ArtifactReference {
        path: process_path
            .strip_prefix(root.path())?
            .to_str()
            .context("witness_process_path_utf8")?
            .into(),
        sha256: sha256(raw),
    };
    let result = CodeBehaviorWitness {
        schema:"oev2.code-behavior-witness/1".into(),
        experiment_sha256:request.experiment_sha256,
        driver_sha256:e.driver.sha256.clone(), evaluator_sha256:e.evaluator.sha256.clone(),
        oracle_sha256:oracle.into(), baseline_sha256:e.baseline.manifest_sha256.clone(),
        candidate_sha256:e.candidate.manifest_sha256.clone(),
        execution:if all_actual_processes_complete {"completed"} else if process.status == "process_failed" {"failed"} else {"partial"}.into(),
        comparison, process:reference,
        uncovered:vec!["Fresh local fixed-case execution evidence only; original13494 API behavior and engineering adoption remain unknown".into(),
            "Same-user bounded process identity, not hostile-code OS sandbox or reproducible compiler proof".into(),
            "Known shared DTO Value representation fidelity and full native/transitive closure unknown".into(),
            "Missing provider usage and total engineering cost unknown".into()],
    };
    write_new(
        &folder.join("behavior-witness.json"),
        &serde_json::to_value(&result)?,
    )?;
    Ok(result)
}
