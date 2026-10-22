//! Typed response records. Transport exit 0 means a complete frame, including
//! business failure/hold replies. Native/engineering acceptance is never implied.
use crate::*;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROBE_RESPONSE_SCHEMA: &str = "cad-agent.nx-probe/v2";
pub const STAGE_RESPONSE_SCHEMA: &str = "cad-agent.nx-stage-response/v2";
pub const STATUS_RESPONSE_SCHEMA: &str = "cad-agent.nx-status-observation/v2";
pub const UNKNOWN_STATUS_RESPONSE_SCHEMA: &str = "cad-agent.nx-status-unknown/v2";
pub const FETCH_RESPONSE_SCHEMA: &str = "cad-agent.nx-fetch/v2";
pub const ERROR_RESPONSE_SCHEMA: &str = "cad-agent.nx-error/v2";

/// Run/status share one schema-dispatched response type. UNKNOWN remains an
/// inspectable observation and is never converted to a failed/completed job.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum StatusObservation {
    Known(StatusResponse),
    Unknown(UnknownStatusResponse),
}
impl<'de> Deserialize<'de> for StatusObservation {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::from_value(Value::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}
impl StatusObservation {
    pub fn from_value(value: Value) -> Result<Self> {
        match value.get("schema").and_then(Value::as_str) {
            Some(STATUS_RESPONSE_SCHEMA) => Ok(Self::Known(serde_json::from_value(value)?)),
            Some(UNKNOWN_STATUS_RESPONSE_SCHEMA) => {
                Ok(Self::Unknown(serde_json::from_value(value)?))
            }
            _ => anyhow::bail!("unsupported status observation schema"),
        }
    }
    /// Use for raw bytes; unlike plain serde parsing, this rejects duplicates.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        crate::decode(bytes)
    }
    pub fn validate_binding(&self, job_id: &JobId, root: &RootIdentity) -> Result<()> {
        match self {
            Self::Known(value) => value.validate_binding(job_id, root),
            Self::Unknown(value) => value.validate_binding(job_id, root),
        }
    }
    pub fn validate_job(&self, job: &JobManifest) -> Result<()> {
        match self {
            Self::Known(value) => value.validate_job(job),
            Self::Unknown(_) => {
                anyhow::bail!("UNKNOWN observation cannot validate a native job outcome")
            }
        }
    }
    pub fn verify_state_bytes(&self, bytes: &[u8]) -> Result<()> {
        match self {
            Self::Known(value) => value.verify_state_bytes(bytes),
            Self::Unknown(value) => value.verify_state_bytes(bytes),
        }
    }
}

fn state_file_bound(identity: &BlobIdentity) -> Result<()> {
    ensure!(
        identity.bytes > 0 && identity.bytes <= MAX_RECORD_BYTES as u64,
        "invalid state file bound"
    );
    Ok(())
}
fn verify_state_bytes(
    identity: &BlobIdentity,
    record: &impl Serialize,
    bytes: &[u8],
) -> Result<()> {
    state_file_bound(identity)?;
    identity.verify(bytes)?;
    ensure!(
        json::strict_json(bytes)? == serde_json::to_value(record)?,
        "state value differs from original bytes"
    );
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProbeResponse {
    pub schema: String,
    pub root: RootIdentity,
    pub controller: ProcessIdentity,
    pub native_execution_started: bool,
    pub stage_run_requires_expected_root: bool,
}
impl ProbeResponse {
    /// Initial probing has no expected root yet; controller image is always
    /// bound to the independent profile pin used by the launch bootstrap.
    pub fn validate_binding(
        &self,
        expected_root: Option<&RootIdentity>,
        controller: &BlobIdentity,
    ) -> Result<()> {
        self.root.validate()?;
        self.controller.validate()?;
        ensure!(
            self.schema == PROBE_RESPONSE_SCHEMA
                && !self.native_execution_started
                && self.stage_run_requires_expected_root,
            "invalid probe capability claim"
        );
        ensure!(
            self.controller.host_id == self.root.host_id && self.controller.image == *controller,
            "probe controller differs"
        );
        if let Some(expected) = expected_root {
            ensure!(self.root == *expected, "probe root differs");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StageResponse {
    pub schema: String,
    pub receipt: StageReceipt,
    pub state: StateRecord,
    pub state_file: BlobIdentity,
}
impl StageResponse {
    pub fn validate_binding(&self, job: &JobManifest, sent_archive: &BlobIdentity) -> Result<()> {
        ensure!(
            self.schema == STAGE_RESPONSE_SCHEMA,
            "stage response schema differs"
        );
        self.receipt.validate(job, sent_archive)?;
        self.state.validate(job)?;
        state_file_bound(&self.state_file)?;
        ensure!(
            self.state.state == State::Staged,
            "stage response is not an unused staged job"
        );
        Ok(())
    }
    pub fn verify_state_bytes(&self, bytes: &[u8]) -> Result<()> {
        verify_state_bytes(&self.state_file, &self.state, bytes)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatusResponse {
    pub schema: String,
    pub job_id: JobId,
    pub root: RootIdentity,
    /// Original parsed v1 or v2 value; original file bytes are separately bound.
    pub state: Value,
    pub state_file: BlobIdentity,
    pub blocks_writes: bool,
    pub hold_reason: String,
    pub legacy_read_only: bool,
    pub automatic_retry: bool,
}
impl StatusResponse {
    /// Bind the status record's shape and identity. This does not regenerate
    /// original file bytes or independently reproduce a remote historical hold
    /// review. Use verify_state_bytes and validate_hold for those extra facts.
    pub fn validate_binding(&self, job_id: &JobId, root: &RootIdentity) -> Result<()> {
        self.root.validate()?;
        state_file_bound(&self.state_file)?;
        checked_label(&self.hold_reason)?;
        ensure!(
            self.schema == STATUS_RESPONSE_SCHEMA
                && self.job_id == *job_id
                && self.root == *root
                && !self.automatic_retry,
            "status identity or retry policy differs"
        );
        ensure!(
            self.state.get("job_id").and_then(Value::as_str) == Some(job_id.as_str()),
            "status state job differs"
        );
        match self.state.get("schema").and_then(Value::as_str) {
            Some(STATE_SCHEMA) | Some(PUBLISHED_STATE_SCHEMA) => {
                ensure!(!self.legacy_read_only, "v2 state mislabeled legacy");
                let state: StateRecord = serde_json::from_value(self.state.clone())?;
                state.validate_shape()?;
                ensure!(
                    state.root == *root && (!state.blocks_writes() || self.blocks_writes),
                    "status hides active or unknown v2 hold"
                );
            }
            Some(LEGACY_STATE_SCHEMA) => {
                ensure!(self.legacy_read_only, "v1 state requires read-only label")
            }
            _ => anyhow::bail!("unknown status state schema"),
        }
        Ok(())
    }
    pub fn verify_state_bytes(&self, bytes: &[u8]) -> Result<()> {
        verify_state_bytes(&self.state_file, &self.state, bytes)
    }
    /// Reuse a same-job hold assessment produced by the existing shared state
    /// or historical-evidence validators. Never infer a hold release from a
    /// terminal string or an arbitrary hold_reason supplied by the peer.
    pub fn validate_hold(&self, expected: &HoldAssessment) -> Result<()> {
        ensure!(
            self.blocks_writes == expected.blocks_writes
                && self.hold_reason == expected.reason
                && self.legacy_read_only == expected.legacy_v1,
            "status hold assessment differs"
        );
        if let Some(state) = &expected.observed_record {
            ensure!(self.state == *state, "status hold source differs");
        } else {
            ensure!(
                expected.blocks_writes,
                "unbound assessment cannot clear hold"
            );
        }
        Ok(())
    }
    /// Run responses also need to match the locally prepared job and its pinned
    /// manifest digest, rather than only the reusable display job-id string.
    pub fn validate_job(&self, job: &JobManifest) -> Result<()> {
        self.validate_binding(&job.job_id, &job.root)?;
        ensure!(
            !self.legacy_read_only,
            "legacy job cannot be a v2 run result"
        );
        let state: StateRecord = serde_json::from_value(self.state.clone())?;
        state.validate(job)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FetchResponse {
    pub schema: String,
    pub job_id: JobId,
    pub root: RootIdentity,
    pub archive: BlobIdentity,
    pub members: Vec<FileEntry>,
    pub state_file: Option<BlobIdentity>,
}
impl FetchResponse {
    /// Verify exact ZIP bytes, all ordinary members and original state bytes.
    /// Fetch may preserve a malformed old state for recovery; this function does
    /// not turn archive integrity into a state transition or engineering pass.
    pub fn validate_binding(
        &self,
        job_id: &JobId,
        root: &RootIdentity,
        archive_bytes: &[u8],
    ) -> Result<std::collections::BTreeMap<String, Vec<u8>>> {
        self.root.validate()?;
        // Empty old state files are useful recovery evidence. Unlike a known
        // parsed status, fetch does not require interpretable/nonempty JSON.
        if let Some(identity) = &self.state_file {
            ensure!(
                identity.bytes <= MAX_RECORD_BYTES as u64,
                "fetched state bytes exceed bound"
            );
        }
        ensure!(
            self.schema == FETCH_RESPONSE_SCHEMA && self.job_id == *job_id && self.root == *root,
            "fetch response identity differs"
        );
        ensure!(
            self.archive.bytes > 0 && self.archive.bytes <= MAX_PAYLOAD_BYTES,
            "invalid fetch archive bound"
        );
        self.archive.verify(archive_bytes)?;
        let members = bundle::decode_members(archive_bytes)?;
        let inventory = members
            .iter()
            .map(|(name, bytes)| FileEntry::from_bytes(name, bytes))
            .collect::<Result<Vec<_>>>()?;
        verify_inventory(&self.members, &inventory)?;
        match (&self.state_file, members.get("state.json")) {
            (Some(identity), Some(bytes)) => identity.verify(bytes)?,
            (None, None) => {}
            _ => anyhow::bail!("fetched state presence differs from receipt"),
        }
        Ok(members)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ErrorResponse {
    pub schema: String,
    pub error: String,
    pub automatic_retry: bool,
}

/// A job directory can exist while its state is missing, unreadable, malformed
/// or from an unknown schema. This is an explicit blocked observation, not a
/// claim that the job does not exist, and never a native completion response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnknownStatusResponse {
    pub schema: String,
    pub job_id: JobId,
    pub root: RootIdentity,
    /// Some only when the adapter actually read the complete bounded raw file.
    /// Empty/malformed JSON bytes remain valid byte evidence here.
    pub state_file: Option<BlobIdentity>,
    pub blocks_writes: bool,
    pub reason: String,
    pub automatic_retry: bool,
}
impl UnknownStatusResponse {
    pub fn validate_binding(&self, job_id: &JobId, root: &RootIdentity) -> Result<()> {
        self.root.validate()?;
        checked_label(&self.reason)?;
        ensure!(
            self.schema == UNKNOWN_STATUS_RESPONSE_SCHEMA
                && self.job_id == *job_id
                && self.root == *root
                && self.blocks_writes
                && !self.automatic_retry,
            "invalid UNKNOWN status identity or hold"
        );
        if let Some(identity) = &self.state_file {
            ensure!(
                identity.bytes <= MAX_RECORD_BYTES as u64,
                "unknown state bytes exceed bound"
            );
        }
        Ok(())
    }
    pub fn verify_state_bytes(&self, bytes: &[u8]) -> Result<()> {
        let identity = self
            .state_file
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("no complete state file was observed"))?;
        ensure!(
            bytes.len() <= MAX_RECORD_BYTES,
            "unknown state bytes exceed bound"
        );
        identity.verify(bytes)
    }
    // Deliberately no validate_job/terminal/reconcile permission. The controller
    // must first recover actual state/dispatch evidence through shared rules.
}
impl ErrorResponse {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == ERROR_RESPONSE_SCHEMA && !self.automatic_retry,
            "invalid error response policy"
        );
        checked_label(&self.error)
    }
}
