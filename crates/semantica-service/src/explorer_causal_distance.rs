//! Source-bound reads, borrowing the existing session and service budget.
use crate::{MAX_FRAME, McpService, failure, require_contract};
use semantica_contracts::{
    SemanticExplorerCausalDistanceQuery, SemanticExplorerCausalDistanceRequest,
    SemanticExplorerCausalDistanceResult, SemanticRuntimeFailure, SemanticServiceBudgetCharge,
};
use std::time::Instant;
fn timely(deadline: Instant) -> Result<(), SemanticRuntimeFailure> {
    if Instant::now() >= deadline {
        Err(failure(
            "explorer_causal_distance_deadline_exceeded",
            "deadline",
        ))
    } else {
        Ok(())
    }
}
impl McpService<'_> {
    pub fn execute_causal_distance(
        &self,
        input: &SemanticExplorerCausalDistanceRequest,
        deadline: Instant,
    ) -> Result<SemanticExplorerCausalDistanceResult, SemanticRuntimeFailure> {
        self.execute_causal_distance_accounted(input, deadline, false)
    }
    pub(super) fn causal_distance_request(
        &self,
        query: SemanticExplorerCausalDistanceQuery,
        deadline: Instant,
    ) -> Result<SemanticExplorerCausalDistanceRequest, SemanticRuntimeFailure> {
        timely(deadline)?;
        let mut session = self
            .session
            .try_lock()
            .map_err(|_| failure("service_session_unavailable", "session"))?;
        let snapshot = session
            .snapshot()
            .map_err(|_| failure("service_snapshot_failed", "session"))?;
        timely(deadline)?;
        Ok(SemanticExplorerCausalDistanceRequest {
            schema: "semantica.explorer-causal-distance-request/2".into(),
            expected_revision: session.revision(),
            expected_dataset_sha256: snapshot.dataset_sha256,
            query,
        })
    }
    /// HTTP framing and the shared reply writer own exact wire byte charges.
    /// Direct typed calls charge their serialized request and result once here.
    pub(super) fn execute_causal_distance_accounted(
        &self,
        input: &SemanticExplorerCausalDistanceRequest,
        deadline: Instant,
        http_accounted: bool,
    ) -> Result<SemanticExplorerCausalDistanceResult, SemanticRuntimeFailure> {
        timely(deadline)?;
        require_contract(input.validate())?;
        require_contract(input.query.validate())?;
        let bytes = serde_json::to_vec(input)
            .map_err(|_| failure("service_contract_invalid", "contract"))?
            .len();
        if bytes > MAX_FRAME {
            return Err(failure("service_frame_budget_exceeded", "budget"));
        }
        self.budget.charge(&SemanticServiceBudgetCharge {
            kind: "query".into(),
            bytes: if http_accounted { 0 } else { bytes as u64 },
        })?;
        let mut session = self
            .session
            .try_lock()
            .map_err(|_| failure("service_session_unavailable", "session"))?;
        if session.revision() != input.expected_revision {
            return Err(failure("service_revision_mismatch", "source"));
        }
        let before = session
            .snapshot()
            .map_err(|_| failure("service_snapshot_failed", "session"))?;
        if before.dataset_sha256 != input.expected_dataset_sha256 {
            return Err(failure("service_dataset_identity_mismatch", "source"));
        }
        timely(deadline)?;
        let result =
            semantica_runtime::explorer_causal_distance::execute(&session, &input.query, deadline)
                .map_err(|e| failure(&e.to_string(), "query"));
        if session.revision() != input.expected_revision
            || session
                .snapshot()
                .map_err(|_| failure("service_snapshot_failed", "session"))?
                != before
        {
            return Err(failure(
                "explorer_causal_distance_dataset_changed",
                "source",
            ));
        }
        timely(deadline)?;
        let result = result?;
        require_contract(result.validate())?;
        if result.status != 200 {
            return Err(failure(
                "explorer_causal_distance_response_invalid",
                "result",
            ));
        }
        if !http_accounted {
            let bytes = serde_json::to_vec(&result)
                .map_err(|_| failure("service_result_invalid", "result"))?
                .len();
            self.budget.charge(&SemanticServiceBudgetCharge {
                kind: "response".into(),
                bytes: bytes as u64,
            })?;
        }
        timely(deadline)?;
        Ok(result)
    }
}
