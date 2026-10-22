use crate::*;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const WORKER_SCHEMA: &str = "cad-agent.nx-native-worker/v2";
pub const JOB_SCHEMA: &str = "cad-agent.nx-direct-job/v2";
pub const STAGE_SCHEMA: &str = "cad-agent.nx-direct-stage/v2";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capability {
    pub action: String,
    pub params_schema: String,
}

/// A reviewed native/Rust extension, not a script interpreter or OS sandbox.
/// All runtime payload files and build/source files are included in job closure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerManifest {
    pub schema: String,
    pub worker_id: String,
    pub version: String,
    pub target: String,
    pub executable: FileEntry,
    pub runtime_files: Vec<FileEntry>,
    pub source_files: Vec<FileEntry>,
    pub build_recipe: FileEntry,
    pub toolchain: String,
    pub required_runtime: RuntimeIdentity,
    pub capabilities: Vec<Capability>,
}
impl WorkerManifest {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema == WORKER_SCHEMA, "worker schema mismatch");
        for value in [
            &self.worker_id,
            &self.version,
            &self.target,
            &self.toolchain,
        ] {
            checked_label(value)?;
        }
        ensure!(
            [
                "x86_64-pc-windows-gnu",
                "x86_64-pc-windows-msvc",
                "aarch64-pc-windows-msvc"
            ]
            .contains(&self.target.as_str()),
            "unknown Windows worker target"
        );
        ensure!(
            self.executable.path.as_str().starts_with("worker/")
                && self.executable.path.as_str().ends_with(".exe")
                && self.executable.identity.bytes > 0,
            "worker must be an explicit native executable"
        );
        ensure!(
            !self.source_files.is_empty(),
            "missing worker source closure"
        );
        for file in &self.runtime_files {
            ensure!(
                file.path.as_str().starts_with("worker/"),
                "worker runtime path outside worker"
            );
        }
        for file in &self.source_files {
            ensure!(
                file.path.as_str().starts_with("sources/"),
                "worker source path outside sources"
            );
        }
        ensure!(
            self.build_recipe.path.as_str().starts_with("build/"),
            "missing build recipe path"
        );
        validate_inventory(&self.payload_files())?;
        self.required_runtime.validate()?;
        ensure!(
            !self.capabilities.is_empty() && self.capabilities.len() <= 256,
            "invalid capability count"
        );
        let mut actions = BTreeSet::new();
        for cap in &self.capabilities {
            checked_token(&cap.action)?;
            checked_token(&cap.params_schema)?;
            ensure!(actions.insert(&cap.action), "duplicate worker action");
        }
        Ok(())
    }
    pub fn payload_files(&self) -> Vec<FileEntry> {
        let mut files = vec![self.executable.clone(), self.build_recipe.clone()];
        files.extend(self.runtime_files.clone());
        files.extend(self.source_files.clone());
        files
    }
    pub fn digest(&self) -> Result<Digest> {
        self.validate()?;
        record_digest(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobManifest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capacity: Option<NativeCapacity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auxiliary_capacity: Option<NativeAuxiliaryCapacity>,
    pub schema: String,
    pub job_id: JobId,
    pub root: RootIdentity,
    pub worker: WorkerManifest,
    pub action: String,
    pub params_schema: String,
    pub params: FileEntry,
    pub inputs: Vec<FileEntry>,
}
impl JobManifest {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            matches!(
                (&*self.schema, &self.capacity),
                (JOB_SCHEMA, None) | (LARGE_JOB_SCHEMA, Some(_))
            ),
            "job schema mismatch; v1 is read-only"
        );
        ensure!(
            self.auxiliary_capacity.is_none() || self.schema == LARGE_JOB_SCHEMA,
            "native_auxiliary_capacity_requires_v3"
        );
        if let Some(capacity) = &self.auxiliary_capacity {
            capacity.validate()?;
        }
        self.root.validate()?;
        self.worker.validate()?;
        ensure!(
            self.worker
                .capabilities
                .iter()
                .any(|c| c.action == self.action && c.params_schema == self.params_schema),
            "unsupported worker action or parameter schema"
        );
        ensure!(
            self.params.path.as_str() == "params.json",
            "params path must be params.json"
        );
        for file in &self.inputs {
            ensure!(
                file.path.as_str().starts_with("inputs/"),
                "input path outside inputs"
            );
        }
        self.validate_inventory_capacity(&self.payload_files(), false)?;
        ensure!(
            serde_json::to_vec(self)?.len() <= MAX_RECORD_BYTES,
            "native_job_record_bound"
        );
        Ok(())
    }
    /// Manifest itself is transferred separately under the protocol envelope;
    /// its identity is digest(). This list includes every other staged file.
    pub fn payload_files(&self) -> Vec<FileEntry> {
        let mut files = self.worker.payload_files();
        files.push(self.params.clone());
        files.extend(self.inputs.clone());
        files
    }
    pub fn input_files(&self) -> Vec<FileEntry> {
        let mut files = self.inputs.clone();
        files.push(self.params.clone());
        files
    }
    pub fn digest(&self) -> Result<Digest> {
        self.validate()?;
        record_digest(self)
    }
    pub fn input_digest(&self) -> Result<Digest> {
        self.validate()?;
        {
            let mut files = self.input_files();
            files.sort_by(|a, b| a.path.cmp(&b.path));
            record_digest(&files)
        }
    }
    pub fn verify_payload(&self, observed: &[FileEntry]) -> Result<()> {
        self.validate()?;
        {
            self.validate_inventory_capacity(observed, false)?;
            let mut expected = self.payload_files();
            let mut actual = observed.to_vec();
            expected.sort_by(|a, b| a.path.cmp(&b.path));
            actual.sort_by(|a, b| a.path.cmp(&b.path));
            ensure!(expected == actual, "file closure differs");
            Ok(())
        }
    }
    pub fn validate_parameters(&self, raw: &[u8]) -> Result<serde_json::Value> {
        self.validate()?;
        self.params.identity.verify(raw)?;
        let value = json::strict_json(raw)?;
        ensure!(
            value.is_object()
                && value.get("schema").and_then(|v| v.as_str()) == Some(&self.params_schema),
            "parameter schema mismatch"
        );
        // Worker capability owns its parameter-field validation, before any SDK
        // operation. The protocol only binds the schema and exact source bytes.
        Ok(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StageReceipt {
    pub schema: String,
    pub job_id: JobId,
    pub root: RootIdentity,
    pub manifest_digest: Digest,
    pub payload_digest: Digest,
    pub archive: BlobIdentity,
}
impl StageReceipt {
    pub fn validate(&self, job: &JobManifest, sent_archive: &BlobIdentity) -> Result<()> {
        ensure!(
            self.schema == STAGE_SCHEMA
                && self.job_id == job.job_id
                && self.root == job.root
                && self.manifest_digest == job.digest()?
                && self.payload_digest == inventory_digest(&job.payload_files())?
                && self.archive == *sent_archive,
            "stage acknowledgement binding differs"
        );
        Ok(())
    }
}
