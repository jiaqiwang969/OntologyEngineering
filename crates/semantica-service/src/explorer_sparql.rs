//! Source-bound query composition shared by application transports.
use crate::{McpService, failure, rdf_format, require_contract};
use semantica_contracts::{
    SemanticExplorerSparqlExecution, SemanticExplorerSparqlJob, SemanticRuntimeFailure,
    SemanticServiceBudgetCharge,
};
use std::time::Instant;

impl McpService<'_> {
    /// Borrow the sole authoritative session while a host-supervised child queries
    /// a disposable snapshot. The host supplies the original absolute deadline
    /// and process supervisor; this method cannot preempt an arbitrary callback.
    /// Input is charged here. The transport must charge its exact output bytes,
    /// including framing, before delivering them. No response is published here.
    pub fn execute_explorer_sparql<F>(
        &self,
        query: &str,
        deadline: Instant,
        execute: F,
    ) -> Result<SemanticExplorerSparqlExecution, SemanticRuntimeFailure>
    where
        F: FnOnce(
            &SemanticExplorerSparqlJob,
            Instant,
        ) -> Result<SemanticExplorerSparqlExecution, SemanticRuntimeFailure>,
    {
        self.execute_explorer_sparql_accounted(query, deadline, execute, false)
    }
    pub(super) fn execute_explorer_sparql_accounted<F>(
        &self,
        query: &str,
        deadline: Instant,
        execute: F,
        input_already_charged: bool,
    ) -> Result<SemanticExplorerSparqlExecution, SemanticRuntimeFailure>
    where
        F: FnOnce(
            &SemanticExplorerSparqlJob,
            Instant,
        ) -> Result<SemanticExplorerSparqlExecution, SemanticRuntimeFailure>,
    {
        self.execute_explorer_sparql_wire(
            query,
            deadline,
            |job, _, deadline| execute(job, deadline),
            input_already_charged,
            false,
        )
    }
    pub fn execute_explorer_sparql_v2<F>(
        &self,
        query: &str,
        deadline: Instant,
        execute: F,
    ) -> Result<SemanticExplorerSparqlExecution, SemanticRuntimeFailure>
    where
        F: FnOnce(
            &semantica_contracts::SemanticExplorerSparqlJobV2,
            Instant,
        ) -> Result<SemanticExplorerSparqlExecution, SemanticRuntimeFailure>,
    {
        self.execute_explorer_sparql_v2_accounted(query, deadline, execute, false)
    }
    pub(super) fn execute_explorer_sparql_v2_accounted<F>(
        &self,
        query: &str,
        deadline: Instant,
        execute: F,
        input_already_charged: bool,
    ) -> Result<SemanticExplorerSparqlExecution, SemanticRuntimeFailure>
    where
        F: FnOnce(
            &semantica_contracts::SemanticExplorerSparqlJobV2,
            Instant,
        ) -> Result<SemanticExplorerSparqlExecution, SemanticRuntimeFailure>,
    {
        self.execute_explorer_sparql_wire(
            query,
            deadline,
            |job, resources, deadline| {
                let envelope = semantica_contracts::SemanticExplorerSparqlJobV2 {
                    schema: "semantica.explorer-sparql-job/2".into(),
                    resources: resources.clone(),
                    job: job.clone(),
                };
                execute(&envelope, deadline)
            },
            input_already_charged,
            true,
        )
    }
    fn execute_explorer_sparql_wire<F>(
        &self,
        query: &str,
        deadline: Instant,
        execute: F,
        input_already_charged: bool,
        version2: bool,
    ) -> Result<SemanticExplorerSparqlExecution, SemanticRuntimeFailure>
    where
        F: FnOnce(
            &SemanticExplorerSparqlJob,
            &semantica_contracts::SemanticDatasetResourceSelection,
            Instant,
        ) -> Result<SemanticExplorerSparqlExecution, SemanticRuntimeFailure>,
    {
        check_deadline(deadline)?;
        if query.chars().count() > 10_000 {
            return Err(failure("explorer_worker_query_budget", "admission"));
        }
        if !semantica_runtime::explorer_sparql::read_only_query(query)
            .map_err(|_| failure("explorer_worker_read_only_required", "admission"))?
        {
            return Err(failure("explorer_worker_read_only_required", "admission"));
        }
        self.budget.charge(&SemanticServiceBudgetCharge {
            kind: "query".into(),
            bytes: if input_already_charged {
                0
            } else {
                query.len() as u64
            },
        })?;
        // Do not wait indefinitely behind another transport holding this session.
        let mut session = self
            .session
            .try_lock()
            .map_err(|_| failure("service_session_unavailable", "session"))?;
        if !version2 && session.resource_profile() != "semantica.dataset-default/1" {
            return Err(failure(
                "explorer_worker_resource_profile_mismatch",
                "contract",
            ));
        }
        let resources = semantica_contracts::SemanticDatasetResourceSelection {
            schema: "semantica.dataset-resource-selection/1".into(),
            profile: session.resource_profile().into(),
        };
        require_contract(resources.validate())?;
        let before = session
            .snapshot()
            .map_err(|_| failure("service_snapshot_failed", "session"))?;
        let job = SemanticExplorerSparqlJob {
            schema: "semantica.explorer-sparql-job/1".into(),
            dataset_nquads: if version2 {
                before.canonical_nquads.clone()
            } else {
                session
                    .serialize(
                        rdf_format("nquads")
                            .map_err(|_| failure("service_rdf_format_unsupported", "session"))?,
                        None,
                    )
                    .map_err(|_| failure("service_snapshot_failed", "session"))?
            },
            expected_dataset_sha256: before.dataset_sha256.clone(),
            query: query.into(),
            row_limit: 5000,
        };
        require_contract(job.validate())?;
        let raw = if version2 {
            serde_json::to_vec(&semantica_contracts::SemanticExplorerSparqlJobV2 {
                schema: "semantica.explorer-sparql-job/2".into(),
                resources: resources.clone(),
                job: job.clone(),
            })
        } else {
            serde_json::to_vec(&job)
        }
        .map_err(|_| failure("explorer_worker_contract_invalid", "contract"))?;
        if raw.len() > session.worker_input_limit() {
            return Err(failure("explorer_worker_input_budget", "budget"));
        }
        check_deadline(deadline)?;
        let result = execute(&job, &resources, deadline);
        // Check even on child failure; never publish its graph as authoritative.
        if session
            .snapshot()
            .map_err(|_| failure("service_snapshot_failed", "session"))?
            != before
        {
            return Err(failure("explorer_worker_dataset_changed", "session"));
        }
        let result = result?;
        check_deadline(deadline)?;
        require_contract(result.validate())?;
        if result.input_sha256 != semantica_runtime::sha256(&raw)
            || result.dataset_sha256 != before.dataset_sha256
        {
            return Err(failure("explorer_worker_result_invalid", "source"));
        }
        Ok(result)
    }
}

fn check_deadline(deadline: Instant) -> Result<(), SemanticRuntimeFailure> {
    if Instant::now() >= deadline {
        Err(failure("explorer_query_deadline_exceeded", "deadline"))
    } else {
        Ok(())
    }
}
