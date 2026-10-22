//! Source-bound path jobs over the existing session, budget and host supervisor.
use crate::{MAX_FRAME, McpService, failure, require_contract};
use semantica_contracts::{
    SemanticExplorerPathDetailsExecution, SemanticExplorerPathDetailsJob,
    SemanticExplorerPathQuery, SemanticRuntimeFailure, SemanticServiceBudgetCharge,
};
use std::time::Instant;
fn timely(deadline: Instant) -> Result<(), SemanticRuntimeFailure> {
    if Instant::now() >= deadline {
        Err(failure("explorer_query_deadline_exceeded", "deadline"))
    } else {
        Ok(())
    }
}
/// Validate execution shape and query association, not shortest-path optimality.
/// The trusted source-bound worker and independent algorithm checks own that claim.
pub fn validate_execution(
    query: &SemanticExplorerPathQuery,
    work_limit: u64,
    result: &SemanticExplorerPathDetailsExecution,
) -> Result<(), SemanticRuntimeFailure> {
    require_contract(result.validate())?;
    match (result.outcome.as_str(), &result.result, &result.failure) {
        ("completed", Some(details), None) => {
            if details.work_limit != work_limit
                || semantica_runtime::explorer_path_details::validate(details, query).is_err()
            {
                return Err(failure("explorer_worker_result_invalid", "result"));
            }
        }
        ("rejected", None, Some(error)) => {
            require_contract(error.validate())?;
        }
        _ => return Err(failure("explorer_worker_result_invalid", "result")),
    }
    Ok(())
}
impl McpService<'_> {
    /// Charge exact supplied JSON bytes, not a reserialized approximation. The
    /// application transport charges output bytes before delivering any result.
    pub fn execute_explorer_path_details<F>(
        &self,
        raw_query: &[u8],
        work_limit: u64,
        deadline: Instant,
        execute: F,
    ) -> Result<SemanticExplorerPathDetailsExecution, SemanticRuntimeFailure>
    where
        F: FnOnce(
            &SemanticExplorerPathDetailsJob,
            Instant,
        ) -> Result<SemanticExplorerPathDetailsExecution, SemanticRuntimeFailure>,
    {
        self.execute_explorer_path_details_accounted(
            raw_query, work_limit, deadline, execute, false,
        )
    }
    pub(super) fn execute_explorer_path_details_accounted<F>(
        &self,
        raw_query: &[u8],
        work_limit: u64,
        deadline: Instant,
        execute: F,
        already_charged: bool,
    ) -> Result<SemanticExplorerPathDetailsExecution, SemanticRuntimeFailure>
    where
        F: FnOnce(
            &SemanticExplorerPathDetailsJob,
            Instant,
        ) -> Result<SemanticExplorerPathDetailsExecution, SemanticRuntimeFailure>,
    {
        timely(deadline)?;
        if !(1..=10_000_000).contains(&work_limit) {
            return Err(failure("explorer_path_details_budget", "admission"));
        }
        if raw_query.len() > MAX_FRAME {
            return Err(failure("service_frame_budget_exceeded", "admission"));
        }
        let query: SemanticExplorerPathQuery = semantica_contracts::json::strict_json(raw_query)
            .ok()
            .and_then(|v| serde_json::from_value(v).ok())
            .ok_or_else(|| failure("explorer_worker_contract_invalid", "contract"))?;
        require_contract(query.validate())?;
        self.budget.charge(&SemanticServiceBudgetCharge {
            kind: "query".into(),
            bytes: if already_charged {
                0
            } else {
                raw_query.len() as u64
            },
        })?;
        let mut session = self
            .session
            .try_lock()
            .map_err(|_| failure("service_session_unavailable", "session"))?;
        let before = session
            .snapshot()
            .map_err(|_| failure("service_snapshot_failed", "session"))?;
        let job = SemanticExplorerPathDetailsJob {
            schema: "semantica.explorer-path-details-job/1".into(),
            resources: semantica_contracts::SemanticDatasetResourceSelection {
                schema: "semantica.dataset-resource-selection/1".into(),
                profile: session.resource_profile().into(),
            },
            dataset_nquads: before.canonical_nquads.clone(),
            expected_dataset_sha256: before.dataset_sha256.clone(),
            query,
            work_limit,
        };
        require_contract(job.validate())?;
        require_contract(job.resources.validate())?;
        let raw = serde_json::to_vec(&job)
            .map_err(|_| failure("explorer_worker_contract_invalid", "contract"))?;
        if raw.len() > session.worker_input_limit() {
            return Err(failure("explorer_worker_input_budget", "budget"));
        }
        timely(deadline)?;
        let outcome = execute(&job, deadline);
        if session
            .snapshot()
            .map_err(|_| failure("service_snapshot_failed", "session"))?
            != before
        {
            return Err(failure("explorer_worker_dataset_changed", "session"));
        }
        let result = outcome?;
        timely(deadline)?;
        validate_execution(&job.query, job.work_limit, &result)?;
        if result.input_sha256 != semantica_runtime::sha256(&raw)
            || result.dataset_sha256 != before.dataset_sha256
        {
            return Err(failure("explorer_worker_result_invalid", "source"));
        }
        if let Some(error) = result.failure {
            return Err(error);
        }
        Ok(result)
    }
}
