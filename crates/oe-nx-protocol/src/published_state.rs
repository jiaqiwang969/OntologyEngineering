//! Explicit v3 state provenance; observations and durable writes remain owners'
//! responsibilities, and no state transition alone starts a native process.
use crate::{
    published_payload::{PreparedPayload, PublishedPayloadPlan},
    *,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
pub const PUBLISHED_STATE_SCHEMA: &str = "oev2.nx-native-state/3";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublishedStageBinding {
    pub source_plan_digest: Digest,
    pub prepared_payload_digest: Digest,
    pub payload_digest: Digest,
    pub target: RootIdentity,
}
impl PublishedStageBinding {
    pub fn from_prepared(
        job: &JobManifest,
        plan: &PublishedPayloadPlan,
        prepared: &PreparedPayload,
    ) -> Result<Self> {
        prepared.validate(job, plan)?;
        Ok(Self {
            source_plan_digest: plan.digest(job)?,
            prepared_payload_digest: record_digest(prepared)?,
            payload_digest: prepared.payload_digest.clone(),
            target: prepared.target.clone(),
        })
    }
    pub fn validate_root(&self, root: &RootIdentity) -> Result<()> {
        self.target.validate()?;
        ensure!(
            self.target.host_id == root.host_id
                && self.target.volume_id == root.volume_id
                && self.target != *root,
            "published_state_target_binding"
        );
        Ok(())
    }
}
impl StateRecord {
    pub fn mark_staged_published(
        &mut self,
        job: &JobManifest,
        plan: &PublishedPayloadPlan,
        prepared: &PreparedPayload,
    ) -> Result<()> {
        self.validate(job)?;
        ensure!(
            job.schema == LARGE_JOB_SCHEMA && self.state == State::Prepared,
            "published_stage_requires_unused_v3"
        );
        let binding = PublishedStageBinding::from_prepared(job, plan, prepared)?;
        binding.validate_root(&self.root)?;
        self.source_admission = Some(binding);
        self.state = State::Staged;
        self.validate(job)
    }
    pub fn validate_published_stage(
        &self,
        job: &JobManifest,
        plan: &PublishedPayloadPlan,
        prepared: &PreparedPayload,
        observed_target: &RootIdentity,
    ) -> Result<()> {
        self.validate(job)?;
        ensure!(
            job.schema == LARGE_JOB_SCHEMA && self.state != State::Prepared,
            "published_state_requires_staged_v3"
        );
        let expected = PublishedStageBinding::from_prepared(job, plan, prepared)?;
        ensure!(
            self.source_admission.as_ref() == Some(&expected)
                && expected.target == *observed_target,
            "published_state_current_origin_or_directory_differs"
        );
        Ok(())
    }
    pub fn begin_published_dispatch(
        &mut self,
        job: &JobManifest,
        plan: &PublishedPayloadPlan,
        prepared: &PreparedPayload,
        observed_target: &RootIdentity,
        intent: DispatchIntent,
    ) -> Result<()> {
        self.validate_published_stage(job, plan, prepared, observed_target)?;
        self.begin_dispatch_inner(job, intent)
    }
}
