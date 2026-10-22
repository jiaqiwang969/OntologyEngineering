//! Source-bound child execution. Parent process supervision remains required.
use anyhow::{Result, ensure};
use semantica_contracts::{SemanticExplorerSparqlExecution, SemanticExplorerSparqlJob};
use semantica_runtime::{dataset::DatasetSession, explorer_sparql};
use std::io::{Read, Write};

pub(super) fn execute_worker() -> Result<u8> {
    execute_worker_profile(false)
}
pub(super) fn execute_worker_v2() -> Result<u8> {
    execute_worker_profile(true)
}
fn execute_worker_profile(version2: bool) -> Result<u8> {
    let limit = if version2 {
        256 * 1024 * 1024
    } else {
        semantica_runtime::MAX_BYTES
    };
    let mut raw = Vec::new();
    std::io::stdin()
        .lock()
        .take(limit as u64 + 1)
        .read_to_end(&mut raw)?;
    ensure!(raw.len() <= limit, "explorer_worker_input_budget");
    let value = oe_core::json::strict_json(&raw)?;
    let (job, mut session) = if version2 {
        let envelope: semantica_contracts::SemanticExplorerSparqlJobV2 =
            serde_json::from_value(value)?;
        ensure!(
            envelope.validate().is_ok() && envelope.resources.validate().is_ok(),
            "explorer_worker_contract_invalid"
        );
        let session = DatasetSession::with_resources(&envelope.resources)?;
        ensure!(
            raw.len() <= session.worker_input_limit(),
            "explorer_worker_input_budget"
        );
        (envelope.job, session)
    } else {
        (
            serde_json::from_value::<SemanticExplorerSparqlJob>(value)?,
            DatasetSession::new(),
        )
    };
    ensure!(job.validate().is_ok(), "explorer_worker_contract_invalid");
    ensure!(
        job.row_limit <= 5000 && job.query.chars().count() <= 10000,
        "explorer_worker_query_budget"
    );
    ensure!(
        explorer_sparql::read_only_query(&job.query)?,
        "explorer_worker_read_only_required"
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
    eprintln!(
        "{}",
        serde_json::json!({"schema":"semantica.explorer-worker-phase/1","phase":"query_dispatch","input_sha256":semantica_runtime::sha256(&raw)})
    );
    let result =
        explorer_sparql::query_projection_response(&session, &job.query, job.row_limit as usize)?;
    ensure!(
        session.snapshot()? == before,
        "explorer_worker_dataset_changed"
    );
    let result = SemanticExplorerSparqlExecution {
        schema: "semantica.explorer-sparql-execution/1".into(),
        input_sha256: semantica_runtime::sha256(&raw),
        dataset_sha256: before.dataset_sha256,
        result,
    };
    ensure!(result.validate().is_ok(), "explorer_worker_result_invalid");
    let output = serde_json::to_vec(&result)?;
    ensure!(
        output.len() <= semantica_runtime::MAX_BYTES,
        "explorer_worker_output_budget"
    );
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(&output)?;
    stdout.write_all(b"\n")?;
    stdout.flush()?;
    Ok(0)
}

#[derive(clap::Args)]
pub struct ExplorerQueryArgs {
    #[arg(long)]
    pub(super) context_graph: std::path::PathBuf,
    #[arg(long)]
    pub(super) context_graph_sha256: String,
    #[arg(long)]
    pub(super) query_file: std::path::PathBuf,
    #[arg(long)]
    pub(super) query_sha256: String,
    /// New private evidence directory; existing evidence is never overwritten.
    #[arg(long)]
    pub(super) evidence: std::path::PathBuf,
    #[arg(long,default_value_t=30000,value_parser=clap::value_parser!(u64).range(1..=30000))]
    pub(super) timeout_ms: u64,
}

#[derive(clap::Args)]
pub struct ExplorerQueryV2Args {
    #[command(flatten)]
    pub query: ExplorerQueryArgs,
    #[arg(long,value_parser=["semantica.dataset-default/1","semantica.context-50k/1"])]
    pub resource_profile: String,
}

/// Host composition over the existing session, budget and process owner.
/// The child receives a disposable RDF snapshot, never Project authority.
pub(super) fn execute_host(
    root: &oe_core::SkillRoot,
    args: ExplorerQueryArgs,
    session: &mut DatasetSession,
    budget: &semantica_service::NativeServiceBudget,
) -> Result<u8> {
    execute_host_profile(root, args, session, budget, false)
}
pub(super) fn execute_host_v2(
    root: &oe_core::SkillRoot,
    args: ExplorerQueryArgs,
    session: &mut DatasetSession,
    budget: &semantica_service::NativeServiceBudget,
) -> Result<u8> {
    execute_host_profile(root, args, session, budget, true)
}
fn execute_host_profile(
    root: &oe_core::SkillRoot,
    args: ExplorerQueryArgs,
    session: &mut DatasetSession,
    budget: &semantica_service::NativeServiceBudget,
    version2: bool,
) -> Result<u8> {
    use oe_core::read_regular_bytes;
    use semantica_contracts::SemanticServiceBudgetCharge;
    use std::time::{Duration, Instant};
    let deadline = Instant::now() + Duration::from_millis(args.timeout_ms);
    let source = root.expand(&args.context_graph)?;
    let query_path = root.expand(&args.query_file)?;
    let query_bytes = read_regular_bytes(&query_path, 40_000)?;
    ensure!(
        semantica_runtime::sha256(&query_bytes) == args.query_sha256,
        "service_source_identity_mismatch"
    );
    let query = std::str::from_utf8(&query_bytes)?.to_owned();
    ensure!(
        query.chars().count() <= 10_000,
        "explorer_worker_query_budget"
    );
    ensure!(
        explorer_sparql::read_only_query(&query)?,
        "explorer_worker_read_only_required"
    );
    let data = read_regular_bytes(&source, session.dataset_byte_limit())?;
    ensure!(
        semantica_runtime::sha256(&data) == args.context_graph_sha256,
        "service_source_identity_mismatch"
    );
    semantica_runtime::context::load_context_graph(session, &oe_core::json::strict_json(&data)?)?;
    let before = session.snapshot()?;
    let evidence = root.expand(&args.evidence)?;
    let mut output = None;
    let mut capture =
        |execution: Result<(SemanticExplorerSparqlExecution, Vec<u8>)>| match execution {
            Ok((result, bytes)) => {
                output = Some(bytes);
                Ok(result)
            }
            Err(error) => Err(semantica_contracts::SemanticRuntimeFailure {
                code: error.to_string(),
                phase: "process".into(),
                message: "Host-supervised query did not complete".into(),
                retryable: false,
            }),
        };
    let service = semantica_service::McpService::new(session, budget);
    let child = if version2 {
        service.execute_explorer_sparql_v2(&query, deadline, |job, original_deadline| {
            capture(supervise_job_v2(
                root,
                job,
                &evidence,
                original_deadline,
                Duration::from_millis(args.timeout_ms),
            ))
        })
    } else {
        service.execute_explorer_sparql(&query, deadline, |job, original_deadline| {
            capture(supervise_job(
                root,
                job,
                &evidence,
                original_deadline,
                Duration::from_millis(args.timeout_ms),
            ))
        })
    };
    drop(service);
    ensure!(
        read_regular_bytes(&source, session.dataset_byte_limit())? == data
            && read_regular_bytes(&query_path, 40_000)? == query_bytes,
        "service_source_identity_mismatch"
    );
    ensure!(
        session.snapshot()? == before,
        "explorer_worker_dataset_changed"
    );
    child.map_err(|error| anyhow::anyhow!(error.code))?;
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

/// Shared process adapter. Caller owns source/session/budget and the original
/// absolute deadline; this adapter never renews it or accepts late output.
pub(super) fn supervise_job(
    root: &oe_core::SkillRoot,
    job: &SemanticExplorerSparqlJob,
    evidence: &std::path::Path,
    deadline: std::time::Instant,
    timeout: std::time::Duration,
) -> Result<(SemanticExplorerSparqlExecution, Vec<u8>)> {
    ensure!(job.validate().is_ok(), "explorer_worker_contract_invalid");
    supervise_raw(
        root,
        serde_json::to_vec(job)?,
        &job.expected_dataset_sha256,
        evidence,
        deadline,
        timeout,
        "sparql-worker",
        semantica_runtime::MAX_BYTES,
    )
}
pub(super) fn supervise_job_v2(
    root: &oe_core::SkillRoot,
    job: &semantica_contracts::SemanticExplorerSparqlJobV2,
    evidence: &std::path::Path,
    deadline: std::time::Instant,
    timeout: std::time::Duration,
) -> Result<(SemanticExplorerSparqlExecution, Vec<u8>)> {
    ensure!(
        job.validate().is_ok() && job.resources.validate().is_ok() && job.job.validate().is_ok(),
        "explorer_worker_contract_invalid"
    );
    let session = DatasetSession::with_resources(&job.resources)?;
    supervise_raw(
        root,
        serde_json::to_vec(job)?,
        &job.job.expected_dataset_sha256,
        evidence,
        deadline,
        timeout,
        "sparql-worker-v2",
        session.worker_input_limit(),
    )
}
fn supervise_raw(
    root: &oe_core::SkillRoot,
    raw: Vec<u8>,
    expected_dataset_sha256: &str,
    evidence: &std::path::Path,
    deadline: std::time::Instant,
    timeout: std::time::Duration,
    worker: &str,
    input_limit: usize,
) -> Result<(SemanticExplorerSparqlExecution, Vec<u8>)> {
    let expected_input = semantica_runtime::sha256(&raw);
    let output = supervise_wire(
        root,
        raw,
        evidence,
        deadline,
        timeout,
        worker,
        input_limit,
        worker == "sparql-worker-v2",
    )?;
    let result: SemanticExplorerSparqlExecution =
        serde_json::from_value(oe_core::json::strict_json(&output)?)?;
    ensure!(
        result.validate().is_ok()
            && result.input_sha256 == expected_input
            && result.dataset_sha256 == expected_dataset_sha256,
        "explorer_worker_result_invalid"
    );
    ensure!(
        std::time::Instant::now() < deadline,
        "explorer_query_deadline_exceeded"
    );
    Ok((result, output))
}

/// Common ProcessTree transport; operation-specific result validation stays with
/// the caller. Legacy stdin bounds remain selected explicitly by the old caller.
pub(super) fn supervise_wire(
    root: &oe_core::SkillRoot,
    raw: Vec<u8>,
    evidence: &std::path::Path,
    deadline: std::time::Instant,
    timeout: std::time::Duration,
    worker: &str,
    input_limit: usize,
    bounded_input: bool,
) -> Result<Vec<u8>> {
    use oe_core::{
        FileIdentity,
        process::{CommandSpec, run_command_with_stdin_file_deadline},
        write_new_bytes,
    };
    use std::{collections::BTreeMap, time::Instant};
    ensure!(raw.len() <= input_limit, "explorer_worker_input_budget");
    ensure!(
        Instant::now() < deadline,
        "explorer_query_deadline_exceeded"
    );
    ensure!(
        evidence.starts_with(root.path().join("var")),
        "explorer_query_evidence_outside_private_root"
    );
    oe_core::create_new_private_dir(&evidence)?;
    let input = evidence.join("input.json");
    write_new_bytes(&input, &raw)?;
    let identity = FileIdentity {
        bytes: raw.len() as u64,
        sha256: semantica_runtime::sha256(&raw),
    };
    let spec = CommandSpec {
        executable: std::env::current_exe()?,
        arguments: vec![
            "--root".into(),
            root.path().as_os_str().into(),
            "services".into(),
            worker.into(),
        ],
        cwd: root.path().to_path_buf(),
        environment: BTreeMap::new(),
        remove_environment: vec![],
        timeout,
        output_limit: semantica_runtime::MAX_BYTES,
    };
    let observation = if bounded_input {
        oe_core::process::run_command_with_bounded_stdin_file_deadline(
            &spec,
            &evidence.join("process"),
            deadline,
            &input,
            &identity,
            input_limit,
        )?
    } else {
        run_command_with_stdin_file_deadline(
            &spec,
            &evidence.join("process"),
            deadline,
            &input,
            &identity,
        )?
    };
    if observation.status == "timed_out" {
        anyhow::bail!("explorer_query_timed_out");
    }
    ensure!(
        observation.status == "completed"
            && observation.exit_code == Some(0)
            && observation.report["capture_complete"] == true,
        "explorer_query_child_incomplete"
    );
    ensure!(
        Instant::now() < deadline,
        "explorer_query_deadline_exceeded"
    );
    Ok(observation.stdout)
}
