//! Independently reviewed root writer admission, never a job-state transition.
//! The control plane pins a concrete review; fresh OS facts remain mandatory.
use crate::*;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const DISPATCH_WRITER_REVIEW_SCHEMA: &str = "cad-agent.nx-dispatch-writer-release-review/v1";
pub const DISPATCH_WRITER_REVIEW_SCHEMA_V2: &str = "cad-agent.nx-dispatch-writer-release-review/v2";
pub const DISPATCH_WRITER_REVIEW_SCHEMA_V3: &str = "cad-agent.nx-dispatch-writer-release-review/v3";
pub const DISPATCH_WRITER_REVIEW_SCHEMA_V4: &str = "cad-agent.nx-dispatch-writer-release-review/v4";
pub const DISPATCH_WRITER_REVIEW_SCHEMA_V5: &str = "cad-agent.nx-dispatch-writer-release-review/v5";
pub const DISPATCH_WRITER_REVIEW_SCHEMA_V6: &str = "cad-agent.nx-dispatch-writer-release-review/v6";
pub const DISPATCH_WRITER_REVIEW_SET_SCHEMA: &str = "cad-agent.nx-dispatch-writer-release-set/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WriterAdmissionOperation {
    Stage,
    Run,
    Supervise,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DispatchWriterReviewScope {
    WriterHoldReleaseOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewedPreNativeFailure {
    ModulePathStringGuardBeforeExplicitUfOrPartApi,
    InitialControllerReceiptGuardBeforeNativeLoad,
    LegacyStepBodiesBudgetGuardBeforeNativeLoad,
    WorkerRoleIdentityGuardBeforeNativeLoad,
    CylinderWorkerRoleIdentityGuardBeforeNativeLoad,
    SheetDimensionParameterSchemaGuardBeforeNativeLoad,
}

/// The operator pins this bounded container and every referenced original
/// review. It creates no state transition and supplies no OS facts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DispatchWriterReleaseSet {
    pub schema: String,
    pub root: RootIdentity,
    pub reviews: Vec<FileEntry>,
}
impl DispatchWriterReleaseSet {
    pub fn validate(&self, root: &RootIdentity) -> Result<()> {
        root.validate()?;
        ensure!(
            self.root == *root && self.schema == DISPATCH_WRITER_REVIEW_SET_SCHEMA,
            "dispatch review set schema/root differs"
        );
        ensure!(
            (1..=32).contains(&self.reviews.len()),
            "dispatch review set cardinality"
        );
        validate_inventory(&self.reviews)?;
        ensure!(
            self.reviews
                .iter()
                .all(|f| f.identity.bytes > 0 && f.identity.bytes <= MAX_RECORD_BYTES as u64),
            "dispatch review set member bound"
        );
        oe_contracts::NxDispatchWriterReviewSetScope {
            schema: "oev2.nx-dispatch-writer-review-set-scope/1".into(),
            container_schema: self.schema.clone(),
            review_policy: "every_pinned_review_freshly_assessed".into(),
            original_job_state: "UNKNOWN_AFTER_DISPATCH".into(),
            recovery_scope: "new_job_writer_admission_only".into(),
            engineering_adoption: "not_assessed".into(),
        }
        .validate()
        .map_err(|_| anyhow::anyhow!("dispatch review set generated scope differs"))?;
        Ok(())
    }
}
pub struct VerifiedDispatchWriterReleaseSet {
    set: DispatchWriterReleaseSet,
}
impl VerifiedDispatchWriterReleaseSet {
    pub fn new(bytes: &[u8], pin: &Digest, root: &RootIdentity) -> Result<Self> {
        ensure!(Digest::of(bytes) == *pin, "dispatch review set pin differs");
        let set: DispatchWriterReleaseSet = decode(bytes)?;
        set.validate(root)?;
        Ok(Self { set })
    }
    pub fn reviews(&self) -> &[FileEntry] {
        &self.set.reviews
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewedDirectoryIdentity {
    pub path: RelativePath,
    pub identity: RootIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DispatchWriterSnapshot {
    pub directory: RootIdentity,
    pub files: Vec<FileEntry>,
    pub directories: Vec<ReviewedDirectoryIdentity>,
}
impl DispatchWriterSnapshot {
    pub fn validate(&self, root: &RootIdentity) -> Result<()> {
        root.validate()?;
        self.directory.validate()?;
        ensure!(
            self.directory.host_id == root.host_id
                && self.directory.volume_id == root.volume_id
                && self.directory.directory_id != root.directory_id,
            "reviewed job directory binding differs"
        );
        validate_inventory(&self.files)?;
        ensure!(self.directories.len() <= MAX_FILES, "directory bound");
        let files: BTreeSet<_> = self
            .files
            .iter()
            .map(|f| path_collision_key(f.path.as_str()))
            .collect();
        let mut dirs = BTreeSet::new();
        let mut ids = BTreeSet::from([self.directory.directory_id.clone()]);
        for entry in &self.directories {
            entry.identity.validate()?;
            ensure!(
                entry.identity.host_id == root.host_id
                    && entry.identity.volume_id == root.volume_id
                    && entry.identity.directory_id != root.directory_id,
                "directory root volume differs"
            );
            let key = path_collision_key(entry.path.as_str());
            ensure!(
                !files.contains(&key)
                    && dirs.insert(key)
                    && ids.insert(entry.identity.directory_id.clone()),
                "duplicate or aliased directory"
            );
        }
        ensure!(
            dirs.contains("out") && dirs.contains("receipts"),
            "actual out/receipts directories required"
        );
        for name in files.iter().chain(&dirs) {
            for (at, _) in name.match_indices('/') {
                ensure!(dirs.contains(&name[..at]), "incomplete directory closure");
            }
        }
        ensure!(
            !dirs.iter().any(|name| name.starts_with("out/")),
            "pre-native out directory must be empty"
        );
        ensure!(
            !self
                .files
                .iter()
                .any(|f| f.path.as_str().starts_with("out/")),
            "pre-native review cannot adopt any output"
        );
        Ok(())
    }
    pub fn digest(&self) -> Result<Digest> {
        let mut value = self.clone();
        value.files.sort_by(|a, b| a.path.cmp(&b.path));
        value.directories.sort_by(|a, b| a.path.cmp(&b.path));
        record_digest(&value)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DispatchWriterReleaseReview {
    pub schema: String,
    pub scope: DispatchWriterReviewScope,
    pub root: RootIdentity,
    pub job_id: JobId,
    pub dispatch_id: Digest,
    pub manifest_digest: Digest,
    pub worker_manifest_digest: Digest,
    pub worker: ProcessIdentity,
    pub supervisor: ProcessIdentity,
    pub snapshot: DispatchWriterSnapshot,
    pub failure: ReviewedPreNativeFailure,
    /// Source/actual DLL diagnosis reviewed outside this pure identity checker.
    pub source_failure_review: FileEntry,
    pub dll_diagnosis_review: FileEntry,
    pub reviewer: String,
    pub reviewed_at_utc: String,
    pub reason: String,
}
impl DispatchWriterReleaseReview {
    pub fn validate(&self, root: &RootIdentity) -> Result<()> {
        ensure!(
            [
                DISPATCH_WRITER_REVIEW_SCHEMA,
                DISPATCH_WRITER_REVIEW_SCHEMA_V2,
                DISPATCH_WRITER_REVIEW_SCHEMA_V3,
                DISPATCH_WRITER_REVIEW_SCHEMA_V4,
                DISPATCH_WRITER_REVIEW_SCHEMA_V5,
                DISPATCH_WRITER_REVIEW_SCHEMA_V6
            ]
            .contains(&self.schema.as_str())
                && self.root == *root,
            "dispatch review schema/root differs"
        );
        ensure!(
            self.schema != DISPATCH_WRITER_REVIEW_SCHEMA_V4
                || self.failure
                    == ReviewedPreNativeFailure::WorkerRoleIdentityGuardBeforeNativeLoad,
            "new role review version cannot reinterpret an old failure"
        );
        ensure!(
            self.schema != DISPATCH_WRITER_REVIEW_SCHEMA_V5
                || self.failure
                    == ReviewedPreNativeFailure::CylinderWorkerRoleIdentityGuardBeforeNativeLoad,
            "cylinder role review version cannot reinterpret an old failure"
        );
        ensure!(
            self.schema != DISPATCH_WRITER_REVIEW_SCHEMA_V6
                || self.failure
                    == ReviewedPreNativeFailure::SheetDimensionParameterSchemaGuardBeforeNativeLoad,
            "sheet parameter review version cannot reinterpret an old failure"
        );
        if self.failure
            == ReviewedPreNativeFailure::SheetDimensionParameterSchemaGuardBeforeNativeLoad
        {
            ensure!(
                self.schema == DISPATCH_WRITER_REVIEW_SCHEMA_V6,
                "sheet parameter failure requires its own review version"
            );
            oe_contracts::NxSheetDimensionParameterGuardScope {
                schema: "oev2.nx-sheet-dimension-parameter-guard-scope/1".into(),
                review_schema: self.schema.clone(),
                failure_kind: "sheet_dimension_parameter_schema_guard_before_native_load".into(),
                expected_worker_id: "oe-nx-native-acceptance".into(),
                worker_target: "x86_64-pc-windows-msvc".into(),
                worker_executable: "worker/oe-nx-worker.exe".into(),
                job_action: "create_sheet_reference_dimension".into(),
                parameter_schema: "oev2.nx-sheet-dimension-params/1".into(),
                original_reported_schema: "cad-agent.nx-native-acceptance-params/v2".into(),
                original_job_state: "UNKNOWN_AFTER_DISPATCH".into(),
                native_execution: "not_started".into(),
                launch_receipt_phase: "persisted_after_worker_exit".into(),
                recovery_scope: "new_job_writer_admission_only".into(),
                engineering_adoption: "not_assessed".into(),
            }
            .validate()
            .map_err(|_| {
                anyhow::anyhow!("sheet parameter guard requires exact generated review scope")
            })?;
        }
        if self.failure == ReviewedPreNativeFailure::CylinderWorkerRoleIdentityGuardBeforeNativeLoad
        {
            oe_contracts::NxCylinderWorkerRoleGuardScope {
                schema: "oev2.nx-cylinder-worker-role-guard-scope/1".into(),
                review_schema: self.schema.clone(),
                failure_kind: "cylinder_worker_role_identity_guard_before_native_load".into(),
                expected_worker_id: "oe-nx-native-acceptance".into(),
                original_declared_worker_id: "oe-nx-rotating-part".into(),
                worker_target: "x86_64-pc-windows-msvc".into(),
                worker_executable: "worker/oe-nx-worker.exe".into(),
                job_action: "create_cylinder_solid".into(),
                parameter_schema: "oev2.nx-rotating-part-params/1".into(),
                original_job_state: "UNKNOWN_AFTER_DISPATCH".into(),
                native_execution: "not_started".into(),
                launch_receipt_phase: "persisted_after_worker_exit".into(),
                recovery_scope: "new_job_writer_admission_only".into(),
                engineering_adoption: "not_assessed".into(),
            }
            .validate()
            .map_err(|_| anyhow::anyhow!("cylinder role guard requires exact new review scope"))?;
        }
        if self.failure == ReviewedPreNativeFailure::WorkerRoleIdentityGuardBeforeNativeLoad {
            oe_contracts::NxNativeWorkerRoleGuardScope {
                schema: "oev2.nx-native-worker-role-guard-scope/1".into(),
                review_schema: self.schema.clone(),
                failure_kind: "worker_role_identity_guard_before_native_load".into(),
                expected_worker_id: "oe-nx-native-acceptance".into(),
                original_declared_worker_id: "oe-nx-native-assembly".into(),
                original_job_state: "UNKNOWN_AFTER_DISPATCH".into(),
                native_execution: "not_started".into(),
                launch_receipt_phase: "persisted_after_worker_exit".into(),
                recovery_scope: "new_job_writer_admission_only".into(),
                engineering_adoption: "not_assessed".into(),
            }
            .validate()
            .map_err(|_| anyhow::anyhow!("worker role guard requires exact new review scope"))?;
        }
        ensure!(
            self.failure != ReviewedPreNativeFailure::InitialControllerReceiptGuardBeforeNativeLoad
                || matches!(
                    self.schema.as_str(),
                    DISPATCH_WRITER_REVIEW_SCHEMA_V2 | DISPATCH_WRITER_REVIEW_SCHEMA_V3
                ),
            "initial receipt failure requires new review protocol"
        );
        if self.failure == ReviewedPreNativeFailure::LegacyStepBodiesBudgetGuardBeforeNativeLoad {
            oe_contracts::NxNativeParameterGuardScope {
                schema: "oev2.nx-native-parameter-guard-scope/1".into(),
                review_schema: self.schema.clone(),
                failure_kind: "legacy_step_bodies_budget_guard_before_native_load".into(),
                original_job_state: "UNKNOWN_AFTER_DISPATCH".into(),
                native_execution: "not_started".into(),
                launch_receipt_phase: "persisted_after_worker_exit".into(),
                recovery_scope: "new_job_writer_admission_only".into(),
                engineering_adoption: "not_assessed".into(),
            }
            .validate()
            .map_err(|_| anyhow::anyhow!("parameter guard requires exact new review scope"))?;
        }
        self.snapshot.validate(root)?;
        self.worker.validate()?;
        self.supervisor.validate()?;
        ensure!(
            self.worker.host_id == root.host_id
                && self.supervisor.host_id == root.host_id
                && (self.worker.pid != self.supervisor.pid
                    || self.worker.creation_time_100ns != self.supervisor.creation_time_100ns),
            "process host/role binding differs"
        );
        for value in [&self.reviewer, &self.reviewed_at_utc, &self.reason] {
            checked_label(value)?;
        }
        validate_inventory(&[
            self.source_failure_review.clone(),
            self.dll_diagnosis_review.clone(),
        ])?;
        ensure!(
            self.source_failure_review.identity.bytes > 0
                && self.dll_diagnosis_review.identity.bytes > 0,
            "independent source and actual DLL diagnosis references required"
        );
        Ok(())
    }
}

/// OS observation: never deserialize it from a worker's untrusted claims.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DispatchProcessState {
    Absent,
    DifferentCreation { actual_creation_time_100ns: u64 },
    ExitedSameIdentity { exit_code: i32 },
    LiveSameIdentity,
    Uncertain,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DispatchProcessObservation {
    pub expected: ProcessIdentity,
    pub observation: DispatchProcessState,
}
#[derive(Debug, Clone)]
pub struct DispatchWriterFacts {
    pub snapshot: DispatchWriterSnapshot,
    /// Original bytes of the fixed control/event records selected below.
    pub records: BTreeMap<String, Vec<u8>>,
    pub worker: DispatchProcessObservation,
    pub supervisor: DispatchProcessObservation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WriterAdmissionRelease {
    pub schema: String,
    pub review_sha256: Digest,
    pub operation: WriterAdmissionOperation,
    pub target_job_id: JobId,
    pub reviewed_job_id: JobId,
    pub dispatch_id: Digest,
    pub snapshot_digest: Digest,
    pub original_state: BlobIdentity,
    pub worker: DispatchProcessObservation,
    pub supervisor: DispatchProcessObservation,
}

/// No public fields or Deserialize implementation can forge verified status.
pub struct VerifiedDispatchWriterReview {
    review: DispatchWriterReleaseReview,
    pin: Digest,
}
impl VerifiedDispatchWriterReview {
    pub fn new(bytes: &[u8], pin: &Digest, root: &RootIdentity) -> Result<Self> {
        ensure!(Digest::of(bytes) == *pin, "dispatch review pin differs");
        let review: DispatchWriterReleaseReview = decode(bytes)?;
        review.validate(root)?;
        Ok(Self {
            review,
            pin: pin.clone(),
        })
    }
    /// MUST be called before the controller applies its supervise `except`.
    pub fn bind_admission(
        &self,
        _operation: WriterAdmissionOperation,
        target: &JobId,
    ) -> Result<()> {
        ensure!(
            !target
                .as_str()
                .eq_ignore_ascii_case(self.review.job_id.as_str()),
            "writer review never permits its original job as target"
        );
        Ok(())
    }
    pub fn job_id(&self) -> &JobId {
        &self.review.job_id
    }
    pub fn worker(&self) -> &ProcessIdentity {
        &self.review.worker
    }
    pub fn supervisor(&self) -> &ProcessIdentity {
        &self.review.supervisor
    }
    pub fn digest(&self) -> &Digest {
        &self.pin
    }
    pub fn assess(
        &self,
        operation: WriterAdmissionOperation,
        target: &JobId,
        input: &HoldInput,
        facts: &DispatchWriterFacts,
    ) -> Result<WriterAdmissionRelease> {
        self.bind_admission(operation, target)?;
        let review = &self.review;
        facts.snapshot.validate(&review.root)?;
        ensure!(
            facts.snapshot.digest()? == review.snapshot.digest()?,
            "reviewed dispatch snapshot changed"
        );
        let HoldInput::StateBytes(raw_state) = input else {
            anyhow::bail!("pending/missing/uncertain state cannot release writer")
        };
        let bytes = |name: &str| -> Result<&[u8]> {
            let raw = facts
                .records
                .get(name)
                .context("required control bytes missing")?;
            let entry = facts
                .snapshot
                .files
                .iter()
                .find(|f| f.path.as_str() == name)
                .context("control absent from inventory")?;
            entry.identity.verify(raw)?;
            Ok(raw)
        };
        ensure!(
            bytes("state.json")? == raw_state,
            "state observation changed"
        );
        let job: JobManifest = decode(bytes("job.json")?)?;
        job.validate()?;
        ensure!(
            job.job_id == review.job_id
                && job.root == review.root
                && job.digest()? == review.manifest_digest
                && job.worker.digest()? == review.worker_manifest_digest,
            "reviewed job/source binding differs"
        );
        let state: StateRecord = decode(raw_state)?;
        state.validate(&job)?;
        ensure!(
            state.state == State::UnknownAfterDispatch && state.result_digest.is_none(),
            "only unresolved result-less dispatch is eligible"
        );
        let intent: DispatchIntent = decode(bytes("dispatch.json")?)?;
        intent.validate(&job)?;
        ensure!(
            state.dispatch.as_ref() == Some(&intent)
                && intent.dispatch_id == review.dispatch_id
                && state.process.as_ref() == Some(&review.worker)
                && review.worker.image == job.worker.executable.identity,
            "original dispatch/process differs"
        );
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Supervisor {
            schema: String,
            dispatch_id: Digest,
            process: ProcessIdentity,
            launch_method: String,
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Termination {
            schema: String,
            dispatch_id: Digest,
            process: ProcessIdentity,
            observer: ProcessIdentity,
            exit_code: i32,
        }
        let supervisor: Supervisor = decode(bytes("receipts/supervisor-process.json")?)?;
        let term: Termination = decode(bytes("receipts/worker-termination.json")?)?;
        ensure!(
            supervisor.schema == "cad-agent.nx-supervisor-process/v2"
                && supervisor.dispatch_id == intent.dispatch_id
                && supervisor.launch_method == "native_COM_Win32_Process_Create"
                && supervisor.process == review.supervisor
                && supervisor.process.image == intent.controller.image,
            "supervisor source/identity differs"
        );
        ensure!(
            term.schema == "cad-agent.nx-native-termination/v2"
                && term.dispatch_id == intent.dispatch_id
                && term.process == review.worker
                && term.observer == review.supervisor
                && term.exit_code == 1,
            "actual termination does not match reviewed pre-native failure"
        );
        match review.failure {
            ReviewedPreNativeFailure::ModulePathStringGuardBeforeExplicitUfOrPartApi => {
                let event: serde_json::Value =
                    decode(bytes("receipts/0001-source_and_dispatch_verified.json")?)?;
                let observation = event
                    .get("observation")
                    .context("source event observation missing")?;
                ensure!(
                    event["schema"] == "cad-agent.nx-native-event/v2"
                        && event["job_id"] == job.job_id.as_str()
                        && event["sequence"] == 1
                        && event["phase"] == "source_and_dispatch_verified"
                        && observation["dispatch_id"] == intent.dispatch_id.as_str()
                        && observation["manifest_digest"] == review.manifest_digest.as_str()
                        && observation["worker_manifest_digest"]
                            == review.worker_manifest_digest.as_str()
                        && observation["input_digest"] == job.input_digest()?.as_str()
                        && serde_json::from_value::<ProcessIdentity>(
                            observation["process"].clone()
                        )? == review.worker
                        && serde_json::from_value::<RuntimeIdentity>(
                            observation["runtime_files_verified"].clone()
                        )? == job.worker.required_runtime,
                    "source/dispatch event binding differs"
                );
                ensure!(
                    !facts
                        .records
                        .contains_key("receipts/worker-launch-environment.json"),
                    "old loader review does not cover additional launch diagnostic"
                );
            }
            ReviewedPreNativeFailure::InitialControllerReceiptGuardBeforeNativeLoad
            | ReviewedPreNativeFailure::LegacyStepBodiesBudgetGuardBeforeNativeLoad
            | ReviewedPreNativeFailure::WorkerRoleIdentityGuardBeforeNativeLoad
            | ReviewedPreNativeFailure::CylinderWorkerRoleIdentityGuardBeforeNativeLoad
            | ReviewedPreNativeFailure::SheetDimensionParameterSchemaGuardBeforeNativeLoad => {
                ensure!(
                    !facts
                        .records
                        .contains_key("receipts/0001-source_and_dispatch_verified.json"),
                    "source/native event exceeds initial-receipt failure scope"
                );
                let launch: serde_json::Value =
                    decode(bytes("receipts/worker-launch-environment.json")?)?;
                let expected_launch_scope = match review.failure {
                    ReviewedPreNativeFailure::InitialControllerReceiptGuardBeforeNativeLoad =>
                        "worker child PATH only; parent unchanged; NXBIN variable inherited",
                    ReviewedPreNativeFailure::LegacyStepBodiesBudgetGuardBeforeNativeLoad
                    | ReviewedPreNativeFailure::WorkerRoleIdentityGuardBeforeNativeLoad
                    | ReviewedPreNativeFailure::CylinderWorkerRoleIdentityGuardBeforeNativeLoad
                    | ReviewedPreNativeFailure::SheetDimensionParameterSchemaGuardBeforeNativeLoad =>
                        "worker child PATH only; parent unchanged; NXBIN variable inherited; persisted after worker exit to retain the closed initial-receipt contract",
                    _ => unreachable!(),
                };
                ensure!(
                    launch["schema"] == "oev2.nx-worker-launch-environment/1"
                        && launch["scope"] == expected_launch_scope
                        && launch["root_cause"]
                            == "launch-environment hypothesis; sole cause not established"
                        && launch["nx_bin"]
                            .as_str()
                            .is_some_and(|s| s.ends_with("NXBIN"))
                        && launch["child_path_sha256"]
                            .as_str()
                            .is_some_and(|s| Digest::new(s).is_ok()),
                    "launch diagnostic is not the explicitly reviewed initial-receipt variant"
                );
                // Complete source payload, actual terminated worker, empty out,
                // exact stderr and separate source review establish this scope.
                // No native result or old state is changed or accepted.
                if matches!(review.failure, ReviewedPreNativeFailure::LegacyStepBodiesBudgetGuardBeforeNativeLoad
                    | ReviewedPreNativeFailure::WorkerRoleIdentityGuardBeforeNativeLoad
                    | ReviewedPreNativeFailure::CylinderWorkerRoleIdentityGuardBeforeNativeLoad
                    | ReviewedPreNativeFailure::SheetDimensionParameterSchemaGuardBeforeNativeLoad) {
                    ensure!(launch["job_id"] == job.job_id.as_str()
                        && launch["dispatch_id"] == intent.dispatch_id.as_str(),
                        "after-exit launch diagnostic job/dispatch differs");
                }
                if review.failure
                    == ReviewedPreNativeFailure::LegacyStepBodiesBudgetGuardBeforeNativeLoad
                {
                    let parameters: serde_json::Value = decode(bytes("params.json")?)?;
                    let budget = parameters["maximum_bodies"]
                        .as_u64()
                        .context("reviewed budget missing")?;
                    ensure!(
                        job.action == "import_step_bodies"
                            && job.params_schema == "cad-agent.nx-native-acceptance-params/v2"
                            && parameters["schema"] == job.params_schema
                            && parameters["action"] == job.action
                            && parameters["expected_units"] == "mm"
                            && !(1..=128).contains(&budget),
                        "parameter guard is not the exact original legacy budget failure"
                    );
                }
                if review.failure
                    == ReviewedPreNativeFailure::WorkerRoleIdentityGuardBeforeNativeLoad
                {
                    let parameters: serde_json::Value = decode(bytes("params.json")?)?;
                    ensure!(
                        job.worker.worker_id == "oe-nx-native-assembly"
                            && job.worker.target == "x86_64-pc-windows-msvc"
                            && job.worker.executable.path.as_str() == "worker/oe-nx-worker.exe"
                            && job.action == "create_assembly_fixture"
                            && job.params_schema == "cad-agent.nx-native-assembly-params/v1"
                            && parameters["schema"] == job.params_schema
                            && parameters["action"] == job.action,
                        "worker role guard is not the exact original assembly role failure"
                    );
                }
                if review.failure
                    == ReviewedPreNativeFailure::SheetDimensionParameterSchemaGuardBeforeNativeLoad
                {
                    let parameters: serde_json::Value = decode(bytes("params.json")?)?;
                    ensure!(
                        job.worker.worker_id == "oe-nx-native-acceptance"
                            && job.worker.target == "x86_64-pc-windows-msvc"
                            && job.worker.executable.path.as_str() == "worker/oe-nx-worker.exe"
                            && job.action == "create_sheet_reference_dimension"
                            && job.params_schema == "oev2.nx-sheet-dimension-params/1"
                            && parameters["schema"] == job.params_schema
                            && parameters["action"] == job.action,
                        "sheet parameter guard is not the exact original schema reporting failure"
                    );
                }
                if review.failure
                    == ReviewedPreNativeFailure::CylinderWorkerRoleIdentityGuardBeforeNativeLoad
                {
                    let parameters: serde_json::Value = decode(bytes("params.json")?)?;
                    ensure!(
                        job.worker.worker_id == "oe-nx-rotating-part"
                            && job.worker.target == "x86_64-pc-windows-msvc"
                            && job.worker.executable.path.as_str() == "worker/oe-nx-worker.exe"
                            && job.action == "create_cylinder_solid"
                            && job.params_schema == "oev2.nx-rotating-part-params/1"
                            && parameters["schema"] == job.params_schema
                            && parameters["action"] == job.action,
                        "cylinder role guard is not the exact original cylinder role failure"
                    );
                }
            }
        }
        ensure!(
            bytes("receipts/worker.stdout")?.is_empty(),
            "unexpected worker stdout"
        );
        let stderr = std::str::from_utf8(bytes("receipts/worker.stderr")?)?;
        ensure!(
            stderr.trim_end_matches(['\r', '\n']) == match review.failure {
                ReviewedPreNativeFailure::SheetDimensionParameterSchemaGuardBeforeNativeLoad =>
                    "native_worker_failed:native_action_parameters_differ",
                ReviewedPreNativeFailure::ModulePathStringGuardBeforeExplicitUfOrPartApi =>
                    "native_worker_failed:native_loaded_module_path_differs",
                ReviewedPreNativeFailure::InitialControllerReceiptGuardBeforeNativeLoad =>
                    "native_worker_failed:unknown_initial_controller_receipt",
                ReviewedPreNativeFailure::LegacyStepBodiesBudgetGuardBeforeNativeLoad =>
                    "native_worker_failed:step_bodies_observation_scope_invalid",
                ReviewedPreNativeFailure::WorkerRoleIdentityGuardBeforeNativeLoad
                | ReviewedPreNativeFailure::CylinderWorkerRoleIdentityGuardBeforeNativeLoad =>
                    "native_worker_failed:native_worker_identity_contract_differs",
            },
            "failure is not the independently reviewed pre-native loader error"
        );
        let error: serde_json::Value = decode(bytes("receipts/supervisor-error.json")?)?;
        ensure!(
            error["schema"] == "cad-agent.nx-supervisor-error/v2"
                && error["dispatch_id"] == intent.dispatch_id.as_str()
                && error["error"] == "required record missing"
                && error["worker_killed"] == false
                && error["automatic_retry"] == false,
            "supervisor failure differs"
        );
        let receipt: StageReceipt = decode(bytes("stage-receipt.json")?)?;
        receipt.validate(&job, &receipt.archive)?;
        let payload = job.payload_files();
        let current_payload: Vec<_> = facts
            .snapshot
            .files
            .iter()
            .filter(|f| payload.iter().any(|p| p.path == f.path))
            .cloned()
            .collect();
        job.verify_payload(&current_payload)?;
        let mut allowed: BTreeSet<_> = payload.iter().map(|f| f.path.as_str()).collect();
        allowed.extend(
            CONTROL_RECORDS
                .iter()
                .chain(OPTIONAL_DIAGNOSTIC_RECORDS.iter())
                .copied()
                .filter(|name| {
                    match review.failure {
                    ReviewedPreNativeFailure::ModulePathStringGuardBeforeExplicitUfOrPartApi => {
                        *name != "receipts/worker-launch-environment.json"
                    }
                    ReviewedPreNativeFailure::InitialControllerReceiptGuardBeforeNativeLoad
                    | ReviewedPreNativeFailure::LegacyStepBodiesBudgetGuardBeforeNativeLoad
                    | ReviewedPreNativeFailure::WorkerRoleIdentityGuardBeforeNativeLoad
                    | ReviewedPreNativeFailure::CylinderWorkerRoleIdentityGuardBeforeNativeLoad
            | ReviewedPreNativeFailure::SheetDimensionParameterSchemaGuardBeforeNativeLoad => {
                        *name != "receipts/0001-source_and_dispatch_verified.json"
                    }
                }
                }),
        );
        ensure!(
            facts
                .snapshot
                .files
                .iter()
                .all(|f| allowed.contains(f.path.as_str())),
            "extra control/event/result/output invalidates review"
        );
        inactive(&facts.worker, &review.worker, Some(term.exit_code))?;
        inactive(&facts.supervisor, &review.supervisor, None)?;
        Ok(WriterAdmissionRelease {
            schema: "cad-agent.nx-dispatch-writer-admission-release/v1".into(),
            review_sha256: self.pin.clone(),
            operation,
            target_job_id: target.clone(),
            reviewed_job_id: review.job_id.clone(),
            dispatch_id: intent.dispatch_id,
            snapshot_digest: facts.snapshot.digest()?,
            original_state: BlobIdentity::of(raw_state),
            worker: facts.worker.clone(),
            supervisor: facts.supervisor.clone(),
        })
    }
}
/// All members must bind before any controller scan/supervisor exception.
/// Individual fresh assessments and retained OS guards remain mandatory.
pub fn bind_dispatch_writer_review_set(
    operation: WriterAdmissionOperation,
    target: &JobId,
    reviews: &[&VerifiedDispatchWriterReview],
) -> Result<()> {
    ensure!(
        (1..=32).contains(&reviews.len()),
        "dispatch review set cardinality"
    );
    let mut jobs = BTreeSet::new();
    let mut directories = BTreeSet::new();
    let root = &reviews[0].review.root;
    for review in reviews {
        ensure!(
            review.review.root == *root
                && jobs.insert(review.job_id().as_str().to_ascii_lowercase())
                && directories.insert(review.review.snapshot.directory.directory_id.clone()),
            "dispatch review set duplicate job/directory or mixed root"
        );
        review.bind_admission(operation, target)?;
    }
    Ok(())
}
pub const CONTROL_RECORDS: [&str; 10] = [
    "job.json",
    "state.json",
    "dispatch.json",
    "stage-receipt.json",
    "receipts/supervisor-process.json",
    "receipts/worker-termination.json",
    "receipts/worker.stdout",
    "receipts/worker.stderr",
    "receipts/0001-source_and_dispatch_verified.json",
    "receipts/supervisor-error.json",
];
pub const OPTIONAL_DIAGNOSTIC_RECORDS: [&str; 1] = ["receipts/worker-launch-environment.json"];
fn inactive(
    actual: &DispatchProcessObservation,
    expected: &ProcessIdentity,
    exit: Option<i32>,
) -> Result<()> {
    ensure!(
        actual.expected == *expected,
        "OS observation requested for another process"
    );
    match actual.observation {
        DispatchProcessState::Absent => Ok(()),
        DispatchProcessState::DifferentCreation {
            actual_creation_time_100ns,
        } => {
            ensure!(
                actual_creation_time_100ns >= 116_444_736_000_000_000
                    && actual_creation_time_100ns != expected.creation_time_100ns,
                "PID reuse has no distinct precise creation time"
            );
            Ok(())
        }
        DispatchProcessState::ExitedSameIdentity { exit_code } => {
            ensure!(
                exit.is_none_or(|code| code == exit_code),
                "held exit differs from termination"
            );
            Ok(())
        }
        _ => anyhow::bail!("reviewed process remains live or uncertain"),
    }
}
