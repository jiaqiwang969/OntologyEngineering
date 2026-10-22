//! Explicit published-source staging request; never an execution request.
use crate::{
    published_payload::{PreparedPayload, PublishedPayloadPlan},
    *,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageOperation {
    Prepare,
    Inspect,
    Recover,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublishedStageRequest {
    pub schema: String,
    pub operation: StageOperation,
    pub job: JobManifest,
    pub plan: PublishedPayloadPlan,
    pub source_root_paths: Vec<String>,
}
impl PublishedStageRequest {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == "oev2.nx-published-stage-request/1",
            "published_stage_request_schema"
        );
        self.plan.validate(&self.job)?;
        ensure!(
            self.source_root_paths.len() == self.plan.sources.len(),
            "published_stage_source_locations"
        );
        for path in &self.source_root_paths {
            let p = path.strip_prefix(r"\\?\").unwrap_or(path);
            let b = p.as_bytes();
            ensure!(
                path.len() <= 4096
                    && b.len() >= 3
                    && b[0].is_ascii_alphabetic()
                    && b[1] == b':'
                    && b[2] == b'\\'
                    && !p.contains('/')
                    && !p.chars().any(char::is_control)
                    && p.split('\\').skip(1).all(|c| c != "." && c != ".."),
                "published_stage_local_absolute_path_required"
            );
        }
        ensure!(
            serde_json::to_vec(self)?.len() <= MAX_RECORD_BYTES,
            "published_stage_request_size"
        );
        Ok(())
    }
    pub fn wire(&self) -> Result<Request> {
        self.validate()?;
        Ok(Request::StagePublished {
            request_record: serde_json::to_string(self)?,
        })
    }
}
pub fn decode_request(raw: &str) -> Result<PublishedStageRequest> {
    ensure!(
        raw.len() <= MAX_RECORD_BYTES,
        "published_stage_request_size"
    );
    let r: PublishedStageRequest = decode(raw.as_bytes())?;
    r.validate()?;
    Ok(r)
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublishedStageResponse {
    pub schema: String,
    pub request_digest: Digest,
    pub root: RootIdentity,
    pub prepared: PreparedPayload,
    pub state: StateRecord,
    pub native_execution_started: bool,
}
impl PublishedStageResponse {
    pub fn validate(&self, request: &PublishedStageRequest, root: &RootIdentity) -> Result<()> {
        request.validate()?;
        ensure!(
            self.schema == "oev2.nx-published-stage-response/1"
                && self.request_digest == record_digest(request)?
                && self.root == *root
                && request.job.root == *root
                && !self.native_execution_started
                && self.state.state == State::Staged,
            "published_stage_response_binding"
        );
        self.state.validate_published_stage(
            &request.job,
            &request.plan,
            &self.prepared,
            &self.prepared.target,
        )
    }
}
impl PreparedPayload {
    pub fn for_target(
        job: &JobManifest,
        plan: &PublishedPayloadPlan,
        target: RootIdentity,
    ) -> Result<Self> {
        plan.validate(job)?;
        let mut files = job.payload_files();
        files.sort_by(|a, b| a.path.cmp(&b.path));
        let r = Self {
            schema: "oev2.nx-prepared-payload/1".into(),
            job_manifest_digest: job.digest()?,
            source_plan_digest: plan.digest(job)?,
            target,
            payload_digest: record_digest(&files)?,
            files: files.len(),
            bytes: files.iter().map(|f| f.identity.bytes).sum(),
            job_staged: false,
            native_execution_started: false,
        };
        r.validate(job, plan)?;
        Ok(r)
    }
}
