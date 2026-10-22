//! Path host and disposable worker over the existing source/process owners.
use anyhow::{Result, ensure};
use semantica_contracts::{
    SemanticExplorerPathExecution, SemanticExplorerPathJob, SemanticRuntimeFailure,
    SemanticServiceBudgetCharge,
};
use semantica_runtime::dataset::DatasetSession;
use std::{
    io::{Read, Write},
    time::{Duration, Instant},
};

pub(super) fn core_failure(error: anyhow::Error) -> SemanticRuntimeFailure {
    let text = error.to_string();
    let known = [
        "explorer_path_source_missing",
        "explorer_path_target_missing",
        "explorer_path_negative_weight",
        "explorer_path_weight_invalid",
        "explorer_path_cost_overflow",
        "explorer_path_deadline_exceeded",
        "explorer_path_identifier_budget",
        "explorer_path_query_contract",
        "context_projection_unavailable",
    ];
    SemanticRuntimeFailure {
        code: if known.contains(&text.as_str()) {
            text
        } else {
            "explorer_path_execution_failed".into()
        },
        phase: "path".into(),
        message: "Path query rejected; no path result published".into(),
        retryable: false,
    }
}
pub(super) fn execute_worker() -> Result<u8> {
    let mut raw = Vec::new();
    std::io::stdin()
        .lock()
        .take(256 * 1024 * 1024 + 1)
        .read_to_end(&mut raw)?;
    ensure!(
        raw.len() <= 256 * 1024 * 1024,
        "explorer_worker_input_budget"
    );
    let job: SemanticExplorerPathJob = serde_json::from_value(oe_core::json::strict_json(&raw)?)?;
    ensure!(
        job.validate().is_ok() && job.resources.validate().is_ok() && job.query.validate().is_ok(),
        "explorer_worker_contract_invalid"
    );
    let mut session = DatasetSession::with_resources(&job.resources)?;
    ensure!(
        raw.len() <= session.worker_input_limit(),
        "explorer_worker_input_budget"
    );
    session.load(
        job.dataset_nquads.as_bytes(),
        semantica_service::rdf_format("nquads")?,
        None,
        None,
    )?;
    let before = session.snapshot()?;
    ensure!(
        before.dataset_sha256 == job.expected_dataset_sha256,
        "service_dataset_identity_mismatch"
    );
    let input_sha256 = semantica_runtime::sha256(&raw);
    eprintln!(
        "{}",
        serde_json::json!({"schema":"semantica.explorer-worker-phase/1","phase":"query_dispatch","input_sha256":input_sha256})
    );
    // Defensive cooperative ceiling only. The parent's earlier absolute deadline
    // and ProcessTree cancellation govern total input/preparation/query time.
    let result = semantica_runtime::explorer_path::execute(
        &session,
        &job.query,
        Instant::now() + Duration::from_secs(30),
    );
    ensure!(
        session.snapshot()? == before,
        "explorer_worker_dataset_changed"
    );
    let (outcome, result, failure) = match result {
        Ok(result) => ("completed", Some(result), None),
        Err(error) => ("rejected", None, Some(core_failure(error))),
    };
    let result = SemanticExplorerPathExecution {
        schema: "semantica.explorer-path-execution/1".into(),
        input_sha256,
        dataset_sha256: before.dataset_sha256,
        outcome: outcome.into(),
        result,
        failure,
    };
    semantica_service::explorer_path::validate_execution(&job.query, &result)
        .map_err(|e| anyhow::anyhow!(e.code))?;
    let output = serde_json::to_vec(&result)?;
    ensure!(
        output.len() < semantica_runtime::MAX_BYTES,
        "explorer_worker_output_budget"
    );
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(&output)?;
    stdout.write_all(b"\n")?;
    stdout.flush()?;
    Ok(0)
}
pub(super) fn supervise_job(
    root: &oe_core::SkillRoot,
    job: &SemanticExplorerPathJob,
    evidence: &std::path::Path,
    deadline: Instant,
    timeout: Duration,
) -> Result<(SemanticExplorerPathExecution, Vec<u8>)> {
    ensure!(
        job.validate().is_ok() && job.resources.validate().is_ok() && job.query.validate().is_ok(),
        "explorer_worker_contract_invalid"
    );
    let session = DatasetSession::with_resources(&job.resources)?;
    let raw = serde_json::to_vec(job)?;
    let expected = semantica_runtime::sha256(&raw);
    let output = super::sparql::supervise_wire(
        root,
        raw,
        evidence,
        deadline,
        timeout,
        "path-worker",
        session.worker_input_limit(),
        true,
    )?;
    let result: SemanticExplorerPathExecution =
        serde_json::from_value(oe_core::json::strict_json(&output)?)?;
    semantica_service::explorer_path::validate_execution(&job.query, &result)
        .map_err(|e| anyhow::anyhow!(e.code))?;
    ensure!(
        result.input_sha256 == expected && result.dataset_sha256 == job.expected_dataset_sha256,
        "explorer_worker_result_invalid"
    );
    ensure!(
        Instant::now() < deadline,
        "explorer_query_deadline_exceeded"
    );
    Ok((result, output))
}
pub(super) fn execute_host(
    root: &oe_core::SkillRoot,
    args: super::sparql::ExplorerQueryArgs,
    session: &mut DatasetSession,
    budget: &semantica_service::NativeServiceBudget,
) -> Result<u8> {
    let deadline = Instant::now() + Duration::from_millis(args.timeout_ms);
    let source = root.expand(&args.context_graph)?;
    let query_path = root.expand(&args.query_file)?;
    let query = oe_core::read_regular_bytes(&query_path, semantica_service::MAX_FRAME)?;
    ensure!(
        semantica_runtime::sha256(&query) == args.query_sha256,
        "service_source_identity_mismatch"
    );
    let data = oe_core::read_regular_bytes(&source, session.dataset_byte_limit())?;
    ensure!(
        semantica_runtime::sha256(&data) == args.context_graph_sha256,
        "service_source_identity_mismatch"
    );
    semantica_runtime::context::load_context_graph(session, &oe_core::json::strict_json(&data)?)?;
    let before = session.snapshot()?;
    let evidence = root.expand(&args.evidence)?;
    let mut output = None;
    let service = semantica_service::McpService::new(session, budget);
    let result =
        service.execute_explorer_path(
            &query,
            deadline,
            |job, original_deadline| match supervise_job(
                root,
                job,
                &evidence,
                original_deadline,
                Duration::from_millis(args.timeout_ms),
            ) {
                Ok((result, bytes)) => {
                    output = Some(bytes);
                    Ok(result)
                }
                Err(error) => Err(SemanticRuntimeFailure {
                    code: error.to_string(),
                    phase: "process".into(),
                    message: "Host path execution failed".into(),
                    retryable: false,
                }),
            },
        );
    drop(service);
    ensure!(
        oe_core::read_regular_bytes(&source, session.dataset_byte_limit())? == data
            && oe_core::read_regular_bytes(&query_path, semantica_service::MAX_FRAME)? == query,
        "service_source_identity_mismatch"
    );
    ensure!(
        session.snapshot()? == before,
        "explorer_worker_dataset_changed"
    );
    result.map_err(|e| anyhow::anyhow!(e.code))?;
    let output = output.ok_or_else(|| anyhow::anyhow!("explorer_worker_result_invalid"))?;
    budget
        .charge_claim(&SemanticServiceBudgetCharge {
            kind: "response".into(),
            bytes: output.len() as u64,
        })
        .map_err(|e| anyhow::anyhow!(e.code))?;
    ensure!(
        Instant::now() < deadline,
        "explorer_query_deadline_exceeded"
    );
    std::io::stdout().lock().write_all(&output)?;
    Ok(0)
}
