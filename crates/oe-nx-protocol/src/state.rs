use crate::*;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const STATE_SCHEMA: &str = "cad-agent.nx-direct-state/v2";
pub const DISPATCH_SCHEMA: &str = "cad-agent.nx-direct-dispatch/v2";
pub const RESULT_SCHEMA: &str = "cad-agent.nx-native-result/v2";
pub const LEGACY_STATE_SCHEMA: &str = "cad-agent.nx-direct-state/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum State {
    Prepared,
    Staged,
    Starting,
    Running,
    Completed,
    Failed,
    UnknownAfterDispatch,
}
impl State {
    pub fn blocks_writes(self) -> bool {
        matches!(
            self,
            Self::Starting | Self::Running | Self::UnknownAfterDispatch
        )
    }
    pub fn dispatched(self) -> bool {
        !matches!(self, Self::Prepared | Self::Staged)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DispatchIntent {
    pub schema: String,
    pub job_id: JobId,
    pub root: RootIdentity,
    pub manifest_digest: Digest,
    /// Caller-generated fresh cryptographic nonce, persisted before any spawn.
    pub dispatch_id: Digest,
    pub attempt: u8,
    pub controller: ProcessIdentity,
    pub worker_manifest_digest: Digest,
    pub worker: BlobIdentity,
    pub input_digest: Digest,
}
impl DispatchIntent {
    pub fn new(
        job: &JobManifest,
        controller: ProcessIdentity,
        dispatch_id: Digest,
    ) -> Result<Self> {
        let result = Self {
            schema: DISPATCH_SCHEMA.into(),
            job_id: job.job_id.clone(),
            root: job.root.clone(),
            manifest_digest: job.digest()?,
            dispatch_id,
            attempt: 1,
            controller,
            worker_manifest_digest: job.worker.digest()?,
            worker: job.worker.executable.identity.clone(),
            input_digest: job.input_digest()?,
        };
        result.validate(job)?;
        Ok(result)
    }
    pub fn validate(&self, job: &JobManifest) -> Result<()> {
        self.validate_shape()?;
        ensure!(
            self.job_id == job.job_id
                && self.root == job.root
                && self.manifest_digest == job.digest()?
                && self.worker_manifest_digest == job.worker.digest()?
                && self.worker == job.worker.executable.identity
                && self.input_digest == job.input_digest()?,
            "dispatch binding differs"
        );
        Ok(())
    }
    fn validate_shape(&self) -> Result<()> {
        self.root.validate()?;
        self.controller.validate()?;
        ensure!(
            self.schema == DISPATCH_SCHEMA
                && self.attempt == 1
                && self.controller.host_id == self.root.host_id
                && self.worker.bytes > 0,
            "invalid dispatch intent"
        );
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultOutcome {
    Succeeded,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeResult {
    pub schema: String,
    pub job_id: JobId,
    pub root: RootIdentity,
    pub manifest_digest: Digest,
    pub dispatch_id: Digest,
    pub worker_manifest_digest: Digest,
    pub worker: BlobIdentity,
    pub input_digest: Digest,
    pub process: ProcessIdentity,
    pub runtime: RuntimeIdentity,
    pub action: String,
    pub outcome: ResultOutcome,
    /// Native worker's typed/schema-labelled observations. No protocol rule
    /// treats geometry, units or a provider's statement as engineering approval.
    pub readback_schema: String,
    pub readback: Value,
    pub artifacts: Vec<FileEntry>,
    pub errors: Vec<String>,
    pub unknowns: Vec<String>,
}

/// Actual OS/file observations, distinct from a worker's result claims. A
/// missing exit status is uncertainty, never a successful completion. Inputs
/// include the worker/source/build closure to detect mutation during execution.
#[derive(Debug, Clone)]
pub struct CompletionEvidence {
    pub process: ProcessIdentity,
    pub exit_code: Option<i32>,
    pub runtime: RuntimeIdentity,
    pub staged_payload: Vec<FileEntry>,
    pub artifacts: Vec<FileEntry>,
}
impl NativeResult {
    pub fn validate_binding(
        &self,
        job: &JobManifest,
        intent: &DispatchIntent,
        process: &ProcessIdentity,
        observed: &CompletionEvidence,
    ) -> Result<()> {
        observed.process.validate()?;
        observed.runtime.validate()?;
        ensure!(
            observed.process == *process,
            "native process identity differs"
        );
        ensure!(
            observed.runtime == self.runtime,
            "native runtime identity differs"
        );
        self.validate_recorded_binding(
            job,
            intent,
            process,
            &observed.staged_payload,
            &observed.artifacts,
        )?;
        Ok(())
    }
    /// Verify recorded identities and fetched bytes. This does not constitute
    /// a new OS process, exit-code or loaded-runtime observation.
    pub fn validate_recorded_binding(
        &self,
        job: &JobManifest,
        intent: &DispatchIntent,
        process: &ProcessIdentity,
        staged_payload: &[FileEntry],
        artifacts: &[FileEntry],
    ) -> Result<()> {
        intent.validate(job)?;
        process.validate()?;
        self.process.validate()?;
        self.runtime.validate()?;
        ensure!(
            self.schema == RESULT_SCHEMA
                && self.job_id == job.job_id
                && self.root == job.root
                && self.manifest_digest == job.digest()?
                && self.dispatch_id == intent.dispatch_id
                && self.worker_manifest_digest == job.worker.digest()?
                && self.worker == job.worker.executable.identity
                && self.input_digest == job.input_digest()?
                && self.action == job.action,
            "native result identity differs"
        );
        ensure!(
            self.process == *process
                && process.host_id == job.root.host_id
                && process.image == job.worker.executable.identity,
            "native process identity differs"
        );
        ensure!(
            self.runtime == job.worker.required_runtime,
            "native runtime identity differs"
        );
        job.verify_payload(staged_payload)?;
        for file in &self.artifacts {
            ensure!(
                file.path.as_str().starts_with("out/"),
                "artifact outside out"
            );
        }
        job.verify_artifacts(&self.artifacts, artifacts)?;
        checked_token(&self.readback_schema)?;
        ensure!(
            self.readback.is_object()
                && serde_json::to_vec(&self.readback)?.len() <= MAX_RECORD_BYTES / 2,
            "invalid readback"
        );
        ensure!(
            self.errors.len() <= 256 && self.unknowns.len() <= 256,
            "too many diagnostic records"
        );
        for text in self.errors.iter().chain(&self.unknowns) {
            checked_label(text)?;
        }
        if self.outcome == ResultOutcome::Succeeded {
            ensure!(
                self.errors.is_empty() && self.unknowns.is_empty(),
                "success carries unresolved errors or unknowns"
            );
        }
        if self.outcome == ResultOutcome::Failed {
            ensure!(!self.errors.is_empty(), "failure lacks error evidence");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateRecord {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_admission: Option<PublishedStageBinding>,
    pub schema: String,
    pub job_id: JobId,
    pub root: RootIdentity,
    pub manifest_digest: Digest,
    pub state: State,
    pub dispatch: Option<DispatchIntent>,
    pub process: Option<ProcessIdentity>,
    pub result_digest: Option<Digest>,
    pub reason: Option<String>,
    pub uncertainty_history: Vec<UncertaintyEvent>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UncertaintyEvent {
    pub sequence: u64,
    pub reason: String,
    pub result_digest: Option<Digest>,
}
impl StateRecord {
    pub fn blocks_writes(&self) -> bool {
        self.state.blocks_writes()
            || (self.schema == PUBLISHED_STATE_SCHEMA && self.state == State::Prepared)
    }
    pub fn prepared(job: &JobManifest) -> Result<Self> {
        Ok(Self {
            schema: if job.schema == LARGE_JOB_SCHEMA {
                PUBLISHED_STATE_SCHEMA
            } else {
                STATE_SCHEMA
            }
            .into(),
            source_admission: None,
            job_id: job.job_id.clone(),
            root: job.root.clone(),
            manifest_digest: job.digest()?,
            state: State::Prepared,
            dispatch: None,
            process: None,
            result_digest: None,
            reason: None,
            uncertainty_history: vec![],
        })
    }
    pub fn validate(&self, job: &JobManifest) -> Result<()> {
        self.validate_shape()?;
        ensure!(
            matches!(
                (&*job.schema, &*self.schema),
                (JOB_SCHEMA, STATE_SCHEMA) | (LARGE_JOB_SCHEMA, PUBLISHED_STATE_SCHEMA)
            ),
            "state_job_version_mismatch"
        );
        ensure!(
            self.job_id == job.job_id
                && self.root == job.root
                && self.manifest_digest == job.digest()?,
            "state job binding differs"
        );
        if let Some(intent) = &self.dispatch {
            intent.validate(job)?;
        }
        Ok(())
    }
    pub fn validate_shape(&self) -> Result<()> {
        ensure!(
            matches!(&*self.schema, STATE_SCHEMA | PUBLISHED_STATE_SCHEMA),
            "state schema mismatch"
        );
        if self.schema == STATE_SCHEMA {
            ensure!(
                self.source_admission.is_none(),
                "legacy_state_contains_published_admission"
            );
        } else if self.state == State::Prepared {
            ensure!(
                self.source_admission.is_none(),
                "prepared_state_contains_admission"
            );
        } else {
            self.source_admission
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("published_state_missing_admission"))?
                .validate_root(&self.root)?;
        }

        self.root.validate()?;
        if let Some(reason) = &self.reason {
            checked_label(reason)?;
        }
        for (index, event) in self.uncertainty_history.iter().enumerate() {
            ensure!(
                event.sequence == index as u64 + 1,
                "invalid uncertainty history sequence"
            );
            checked_label(&event.reason)?;
        }
        if !self.state.dispatched() {
            ensure!(
                self.dispatch.is_none()
                    && self.process.is_none()
                    && self.result_digest.is_none()
                    && self.reason.is_none()
                    && self.uncertainty_history.is_empty(),
                "undispatched state has execution evidence"
            );
        } else {
            let intent = self
                .dispatch
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("dispatched state missing intent"))?;
            intent.validate_shape()?;
            ensure!(
                intent.job_id == self.job_id
                    && intent.root == self.root
                    && intent.manifest_digest == self.manifest_digest,
                "state intent differs"
            );
            if let Some(process) = &self.process {
                process.validate()?;
                ensure!(
                    process.host_id == self.root.host_id && process.image == intent.worker,
                    "state process differs"
                );
            }
            match self.state {
                State::Starting => ensure!(
                    self.process.is_none() && self.result_digest.is_none() && self.reason.is_none(),
                    "invalid starting shape"
                ),
                State::Running => ensure!(
                    self.process.is_some() && self.result_digest.is_none() && self.reason.is_none(),
                    "invalid running shape"
                ),
                State::Completed | State::Failed => ensure!(
                    self.process.is_some() && self.result_digest.is_some(),
                    "terminal state missing bound result"
                ),
                State::UnknownAfterDispatch => ensure!(
                    self.reason.is_some()
                        && self
                            .uncertainty_history
                            .last()
                            .is_some_and(|event| Some(&event.reason) == self.reason.as_ref()),
                    "unknown state requires preserved uncertainty evidence"
                ),
                _ => unreachable!(),
            }
        }
        Ok(())
    }
    pub fn mark_staged(
        &mut self,
        job: &JobManifest,
        receipt: &StageReceipt,
        sent_archive: &BlobIdentity,
    ) -> Result<()> {
        self.validate(job)?;
        ensure!(
            self.state == State::Prepared,
            "stage requires unused prepared job"
        );
        ensure!(job.schema == JOB_SCHEMA, "v3_requires_published_stage");
        receipt.validate(job, sent_archive)?;
        self.state = State::Staged;
        Ok(())
    }
    /// Must be atomically persisted and flushed under the shared-root lock
    /// before spawning. Once returned, caller errors must persist UNKNOWN; this
    /// library never permits a second begin_dispatch for the same state record.
    pub fn begin_dispatch(&mut self, job: &JobManifest, intent: DispatchIntent) -> Result<()> {
        ensure!(
            job.schema == JOB_SCHEMA,
            "v3_requires_current_published_stage_review"
        );
        self.begin_dispatch_inner(job, intent)
    }
    pub(crate) fn begin_dispatch_inner(
        &mut self,
        job: &JobManifest,
        intent: DispatchIntent,
    ) -> Result<()> {
        self.validate(job)?;
        ensure!(
            self.state == State::Staged,
            "single-use dispatch requires staged job"
        );
        intent.validate(job)?;
        self.dispatch = Some(intent);
        self.state = State::Starting;
        Ok(())
    }
    pub fn mark_running(&mut self, job: &JobManifest, process: ProcessIdentity) -> Result<()> {
        self.validate(job)?;
        ensure!(self.state == State::Starting, "running requires starting");
        process.validate()?;
        ensure!(
            process.host_id == job.root.host_id && process.image == job.worker.executable.identity,
            "spawned worker identity differs"
        );
        self.process = Some(process);
        self.state = State::Running;
        Ok(())
    }
    pub fn mark_unknown(&mut self, reason: &str) -> Result<()> {
        self.validate_shape()?;
        checked_label(reason)?;
        ensure!(
            matches!(
                self.state,
                State::Starting | State::Running | State::UnknownAfterDispatch
            ),
            "unknown requires dispatch intent"
        );
        self.record_uncertainty(reason);
        Ok(())
    }
    fn record_uncertainty(&mut self, reason: &str) {
        self.state = State::UnknownAfterDispatch;
        self.reason = Some(reason.into());
        self.uncertainty_history.push(UncertaintyEvent {
            sequence: self.uncertainty_history.len() as u64 + 1,
            reason: reason.into(),
            result_digest: self.result_digest.clone(),
        });
    }
    /// Invalid, stale, incomplete or inconsistent outcomes become a durable
    /// hold. A prior UNKNOWN cannot be silently reconciled by this API.
    pub fn finish(
        &mut self,
        job: &JobManifest,
        result: &NativeResult,
        observed: &CompletionEvidence,
    ) -> Result<()> {
        self.validate(job)?;
        ensure!(
            self.state == State::Running,
            "finish requires running; UNKNOWN needs separate explicit reconciliation"
        );
        self.result_digest = Some(record_digest(result)?);
        let bound = result.validate_binding(
            job,
            self.dispatch.as_ref().unwrap(),
            self.process.as_ref().unwrap(),
            observed,
        );
        let (state, reason) = if bound.is_err() {
            (
                State::UnknownAfterDispatch,
                Some("result_or_observation_binding_invalid"),
            )
        } else if observed.exit_code.is_none() {
            (
                State::UnknownAfterDispatch,
                Some("process_exit_not_observed"),
            )
        } else {
            match (result.outcome, observed.exit_code) {
                (ResultOutcome::Succeeded, Some(0)) => (State::Completed, None),
                (ResultOutcome::Failed, Some(_)) => (State::Failed, None),
                _ => (
                    State::UnknownAfterDispatch,
                    Some("outcome_uncertain_or_exit_conflicts"),
                ),
            }
        };
        self.state = state;
        self.reason = reason.map(str::to_owned);
        if let Some(reason) = reason {
            self.record_uncertainty(reason);
        }
        self.validate(job)
    }
    /// Inspect the SAME already-dispatched worker after an interrupted wait.
    /// No spawn/retry permission is created. Original process identity, known
    /// actual exit and every result/file/runtime binding must still match. The
    /// historical UNKNOWN events remain in the record after reconciliation.
    pub fn reconcile(
        &mut self,
        job: &JobManifest,
        result: &NativeResult,
        observed: &CompletionEvidence,
    ) -> Result<bool> {
        self.validate(job)?;
        ensure!(
            self.state == State::UnknownAfterDispatch,
            "reconciliation requires UNKNOWN"
        );
        let Some(process) = self.process.as_ref() else {
            return Ok(false);
        };
        if result
            .validate_binding(job, self.dispatch.as_ref().unwrap(), process, observed)
            .is_err()
        {
            return Ok(false);
        }
        let state = match (result.outcome, observed.exit_code) {
            (ResultOutcome::Succeeded, Some(0)) => State::Completed,
            (ResultOutcome::Failed, Some(_)) => State::Failed,
            _ => return Ok(false),
        };
        self.result_digest = Some(record_digest(result)?);
        self.state = state;
        self.reason = None;
        self.validate(job)?;
        Ok(true)
    }
}

#[derive(Debug, Clone)]
pub enum HoldInput {
    Missing,
    StateBytes(Vec<u8>),
    IoUncertain(String),
    PendingWrite,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HoldAssessment {
    pub blocks_writes: bool,
    pub reason: String,
    pub legacy_v1: bool,
    pub state: Option<String>,
    /// Preserves old v1 status fields for read-only status/fetch. Unknown fields
    /// are displayed as old evidence, never accepted as a v2 execution request.
    pub observed_record: Option<Value>,
}
fn hold(reason: &str) -> HoldAssessment {
    HoldAssessment {
        blocks_writes: true,
        reason: reason.into(),
        legacy_v1: false,
        state: None,
        observed_record: None,
    }
}
pub fn classify_hold(input: &HoldInput) -> HoldAssessment {
    let bytes = match input {
        HoldInput::Missing => return hold("missing_state_requires_inspection"),
        HoldInput::PendingWrite => return hold("pending_durable_state_write"),
        HoldInput::IoUncertain(_) => return hold("state_io_or_directory_identity_uncertain"),
        HoldInput::StateBytes(bytes) => bytes,
    };
    let value = match json::strict_json(bytes) {
        Ok(value) => value,
        Err(_) => return hold("malformed_state"),
    };
    match value.get("schema").and_then(Value::as_str) {
        Some(STATE_SCHEMA) | Some(PUBLISHED_STATE_SCHEMA) => {
            match serde_json::from_value::<StateRecord>(value.clone()) {
                Ok(record) if record.validate_shape().is_ok() => HoldAssessment {
                    blocks_writes: record.blocks_writes(),
                    reason: if record.blocks_writes() {
                        if record.schema == PUBLISHED_STATE_SCHEMA {
                            if record.state == State::Prepared {
                                "unresolved_v3_preparation"
                            } else {
                                "unresolved_v3_dispatch"
                            }
                        } else {
                            "unresolved_v2_dispatch"
                        }
                    } else {
                        if record.schema == PUBLISHED_STATE_SCHEMA {
                            "valid_v3_state"
                        } else {
                            "valid_v2_state"
                        }
                    }
                    .into(),
                    legacy_v1: false,
                    state: value
                        .get("state")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    observed_record: Some(value),
                },
                _ => hold(
                    if value.get("schema").and_then(Value::as_str) == Some(PUBLISHED_STATE_SCHEMA) {
                        "invalid_v3_state"
                    } else {
                        "invalid_v2_state"
                    },
                ),
            }
        }
        Some(LEGACY_STATE_SCHEMA) => {
            let state = value
                .get("state")
                .and_then(Value::as_str)
                .map(str::to_owned);
            // Legacy terminal records can be displayed; they are not promoted
            // to v2 native evidence. Old FAILED can mean a missing result;
            // strings alone cannot clear a root hold, even for old COMPLETED.
            HoldAssessment {
                blocks_writes: true,
                reason: "legacy_v1_requires_process_and_result_evidence".into(),
                legacy_v1: true,
                state,
                observed_record: Some(value),
            }
        }
        _ => hold("unknown_state_schema"),
    }
}
pub fn assess_root_holds(records: &[HoldAssessment]) -> bool {
    records.iter().any(|r| r.blocks_writes)
}

/// Scanner knows the directory entry and its pinned shared-root handle. Bind
/// those observations, so a valid terminal record copied from another job/root
/// cannot clear a hold. Legacy v1 has no root ID; it remains read-only evidence.
pub fn classify_job_hold(
    input: &HoldInput,
    directory_job_id: &JobId,
    root: &RootIdentity,
) -> HoldAssessment {
    let mut assessment = classify_hold(input);
    if let Some(value) = &assessment.observed_record {
        let id_matches =
            value.get("job_id").and_then(Value::as_str) == Some(directory_job_id.as_str());
        let root_matches = assessment.legacy_v1
            || value.get("root").is_some_and(|value| {
                serde_json::from_value::<RootIdentity>(value.clone())
                    .is_ok_and(|identity| identity == *root)
            });
        if !id_matches || !root_matches {
            assessment.blocks_writes = true;
            assessment.reason = "state_directory_or_root_binding_differs".into();
        }
    }
    assessment
}

/// Adapter-observed legacy process termination, including source-bound durable
/// observations from the old controller when the process has already gone.
/// A new live handle is not required for historical jobs. native_start_utc and
/// creation_time_100ns must describe that SAME persisted/observed process;
/// callers may not substitute a reused PID. Missing identity stays interlocked.
#[derive(Debug, Clone)]
pub struct LegacyCompletionEvidence {
    pub native_pid: u32,
    pub creation_time_100ns: u64,
    pub native_start_utc: String,
    pub hostname: String,
    pub exit_code: Option<i32>,
    pub journal_sha256: Digest,
    pub native_result: Vec<u8>,
}
pub fn classify_legacy_completion(
    input: &HoldInput,
    directory_job_id: &JobId,
    root: &RootIdentity,
    evidence: &LegacyCompletionEvidence,
) -> HoldAssessment {
    let mut assessment = classify_job_hold(input, directory_job_id, root);
    if !assessment.legacy_v1 || assessment.reason == "state_directory_or_root_binding_differs" {
        return assessment;
    }
    let Some(state) = assessment.observed_record.as_ref() else {
        return assessment;
    };
    let checked = (|| -> Result<()> {
        ensure!(
            matches!(
                state.get("state").and_then(Value::as_str),
                Some("RUNNING" | "UNKNOWN" | "UNKNOWN_AFTER_DISPATCH" | "COMPLETED" | "FAILED")
            ),
            "legacy state cannot reconcile"
        );
        ensure!(
            evidence.native_pid != 0
                && evidence.creation_time_100ns >= 116_444_736_000_000_000
                && evidence.exit_code.is_some(),
            "legacy process not known terminated"
        );
        ensure!(
            state.get("native_pid").and_then(Value::as_u64) == Some(evidence.native_pid as u64)
                && state.get("native_start_utc").and_then(Value::as_str)
                    == Some(evidence.native_start_utc.as_str())
                && state.get("hostname").and_then(Value::as_str)
                    == Some(evidence.hostname.as_str())
                && state.get("journal_sha256").and_then(Value::as_str)
                    == Some(evidence.journal_sha256.as_str()),
            "legacy process or journal identity differs"
        );
        if let Some(old_exit) = state.get("exit_code") {
            ensure!(
                old_exit.as_i64() == evidence.exit_code.map(i64::from),
                "legacy exit conflict"
            );
        }
        let result = json::strict_json(&evidence.native_result)?;
        ensure!(
            result.get("schema").and_then(Value::as_str) == Some("cad-agent.nx-direct-result/v1")
                && result.get("job_id").and_then(Value::as_str) == Some(directory_job_id.as_str())
                && result.get("pid").and_then(Value::as_u64) == Some(evidence.native_pid as u64)
                && result.get("journal_sha256").and_then(Value::as_str)
                    == Some(evidence.journal_sha256.as_str()),
            "legacy result identity differs"
        );
        let created =
            (evidence.creation_time_100ns - 116_444_736_000_000_000) as f64 / 10_000_000.0;
        let started = result
            .get("started_at")
            .and_then(Value::as_f64)
            .ok_or_else(|| anyhow::anyhow!("legacy start unknown"))?;
        let finished = result
            .get("finished_at")
            .and_then(Value::as_f64)
            .ok_or_else(|| anyhow::anyhow!("legacy finish unknown"))?;
        ensure!(
            started >= created && finished >= started,
            "legacy result predates actual process"
        );
        match result.get("status").and_then(Value::as_str) {
            Some("ok") => ensure!(
                evidence.exit_code == Some(0) && result.get("readback").is_some(),
                "legacy success contradicts exit or missing readback"
            ),
            Some("error") => ensure!(
                result
                    .get("error")
                    .and_then(Value::as_str)
                    .is_some_and(|s| !s.trim().is_empty()),
                "legacy error lacks receipt"
            ),
            _ => anyhow::bail!("legacy outcome unknown"),
        }
        Ok(())
    })();
    if checked.is_ok() {
        assessment.blocks_writes = false;
        assessment.reason = "legacy_process_terminated_and_result_bound_not_v2_acceptance".into();
    }
    assessment
}

/// Closed transport requests. A profile supplies the already pinned shared root;
/// callers cannot replace it through a job request to evade existing holds.
/// The transport is stdin/stdout over an explicitly launched companion, not RPC.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    FetchBatch {
        request_record: String,
    },
    FetchSnapshot {
        job_id: JobId,
        manifest_digest: Digest,
    },
    FetchRange {
        request_record: String,
    },
    StagePublished {
        request_record: String,
    },
    TransferBegin {
        plan_record: String,
    },
    TransferPut {
        job_id: JobId,
        plan_digest: Digest,
        index: u32,
        payload: BlobIdentity,
    },
    TransferPutBatch {
        job_id: JobId,
        plan_digest: Digest,
        index: u32,
        count: u32,
        payload: BlobIdentity,
    },
    TransferReconstruct {
        job_id: JobId,
        plan_digest: Digest,
        output_id: JobId,
    },
    TransferPublish {
        job_id: JobId,
        plan_digest: Digest,
        output_id: JobId,
        publication_id: JobId,
        expected_target: RootIdentity,
    },
    TransferInspectPublication {
        job_id: JobId,
        plan_digest: Digest,
        output_id: JobId,
        publication_id: JobId,
        expected_target: RootIdentity,
    },
    TransferRecoverPublication {
        job_id: JobId,
        plan_digest: Digest,
        output_id: JobId,
        publication_id: JobId,
        expected_target: RootIdentity,
    },
    TransferInspect {
        job_id: JobId,
        plan_digest: Digest,
    },
    TransferCommit {
        job_id: JobId,
        plan_digest: Digest,
    },
    TransferVerify {
        job_id: JobId,
        plan_digest: Digest,
    },
    Probe {},
    Stage {
        job: Box<JobManifest>,
        archive: BlobIdentity,
    },
    Run {
        job_id: JobId,
        manifest_digest: Digest,
        wait_seconds: u16,
    },
    Status {
        job_id: JobId,
    },
    Fetch {
        job_id: JobId,
    },
}
impl Request {
    pub fn validate(&self) -> Result<()> {
        if crate::transfer::is_transfer(self) {
            return crate::transfer::validate_request(self);
        }
        match self {
            Self::FetchBatch { request_record } => {
                let r: crate::large_fetch::FetchBatch = crate::decode(request_record.as_bytes())?;
                r.validate()?;
            }
            Self::FetchRange { request_record } => {
                let r: crate::large_fetch::FetchRange = crate::decode(request_record.as_bytes())?;
                r.validate()?;
            }
            Self::StagePublished { request_record } => {
                crate::published_stage::decode_request(request_record)?;
            }
            Self::Stage { job, archive } => {
                job.require_archive_stage()?;
                ensure!(
                    archive.bytes > 0 && archive.bytes <= MAX_PAYLOAD_BYTES,
                    "archive exceeds bound"
                );
            }
            Self::Run { wait_seconds, .. } => ensure!(
                (1..=300).contains(wait_seconds),
                "wait_seconds out of range"
            ),
            _ => {}
        }
        Ok(())
    }
}
