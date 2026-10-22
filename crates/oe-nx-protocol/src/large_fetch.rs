//! Byte observations and range receipts, never native completion adjudication.
use crate::*;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub const MAX_SNAPSHOT_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_AUX_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_AUX_FILE_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_AUX_FILES: usize = 1088;
pub const MAX_FETCH_FILES: usize = MAX_FILES * 2 + MAX_AUX_FILES;
pub fn file_count_bound(job: &JobManifest) -> Result<usize> {
    let auxiliary = job.auxiliary_bounds()?.2;
    if job.auxiliary_capacity.is_none() {
        return Ok(MAX_FETCH_FILES);
    }
    let input = job.inventory_bounds(false)?.2;
    let output = job.inventory_bounds(true)?.2;
    input
        .checked_add(output)
        .and_then(|n| n.checked_add(auxiliary))
        .ok_or_else(|| anyhow::anyhow!("fetch_file_count_overflow"))
}
pub fn control_name(name: &str) -> bool {
    if [
        "job.json",
        "state.json",
        "source-plan.json",
        "stage-receipt.json",
        "dispatch.json",
        "native-result.json",
        "native-result.pending.json",
        "worker-pre-native-unknown.json",
        ".nx-state-write.pending",
    ]
    .contains(&name)
    {
        return true;
    }
    for (prefix, suffix) in [("stage-recovery-", ".json"), (".nx-state-", ".tmp")] {
        if let Some(s) = name
            .strip_prefix(prefix)
            .and_then(|s| s.strip_suffix(suffix))
        {
            let p = s.split('-').collect::<Vec<_>>();
            if p.len() == 2
                && p.iter().all(|s| {
                    !s.is_empty()
                        && s.bytes().all(|b| b.is_ascii_digit())
                        && s.parse::<u64>().is_ok()
                })
            {
                return true;
            }
        }
    }
    false
}
pub fn file_bound(job: &JobManifest, path: &RelativePath) -> Result<u64> {
    let name = path.as_str();
    if job.payload_files().iter().any(|f| f.path == *path) {
        return Ok(job.inventory_bounds(false)?.1);
    }
    if name.starts_with("out/") {
        return Ok(job.inventory_bounds(true)?.1);
    }
    let auxiliary = job.auxiliary_bounds()?.1;
    if name.starts_with("receipts/") {
        return Ok(auxiliary);
    }
    ensure!(control_name(name), "fetch_path_outside_job_classes");
    Ok((MAX_RECORD_BYTES as u64).min(auxiliary))
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FetchFile {
    pub entry: FileEntry,
    pub volume_id: String,
    pub file_id: String,
}
impl FetchFile {
    pub fn validate(&self) -> Result<()> {
        for id in [&self.volume_id, &self.file_id] {
            ensure!(
                !id.is_empty() && id.len() <= 256 && !id.chars().any(char::is_control),
                "fetch_file_identity_invalid"
            );
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FetchBinding {
    pub root: RootIdentity,
    pub target: RootIdentity,
    pub job_id: JobId,
    pub manifest_digest: Digest,
    pub job_record: BlobIdentity,
    pub state_record: Option<BlobIdentity>,
}
impl FetchBinding {
    pub fn validate(&self) -> Result<()> {
        self.root.validate()?;
        self.target.validate()?;
        ensure!(
            self.target != self.root
                && self.target.host_id == self.root.host_id
                && self.target.volume_id == self.root.volume_id,
            "fetch_target_scope"
        );
        ensure!(
            self.job_record.bytes > 0 && self.job_record.bytes <= MAX_RECORD_BYTES as u64,
            "fetch_job_record_bound"
        );
        if let Some(s) = &self.state_record {
            ensure!(
                s.bytes <= MAX_RECORD_BYTES as u64,
                "fetch_state_record_bound"
            );
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FetchSnapshot {
    pub schema: String,
    pub binding: FetchBinding,
    pub job: JobManifest,
    pub files: Vec<FetchFile>,
    pub directories: Vec<RelativePath>,
    pub qualification: String,
}
impl FetchSnapshot {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == "oev2.nx-fetch-snapshot/1"
                && self.qualification == "byte_observation_only",
            "fetch_snapshot_schema_scope"
        );
        self.binding.validate()?;
        self.job.validate()?;
        ensure!(
            self.job.root == self.binding.root
                && self.job.job_id == self.binding.job_id
                && self.job.digest()? == self.binding.manifest_digest,
            "fetch_snapshot_job_binding"
        );
        let count_bound = file_count_bound(&self.job)?;
        let auxiliary = self.job.auxiliary_bounds()?;
        ensure!(
            self.files.len() <= count_bound && self.directories.len() <= count_bound,
            "fetch_snapshot_count"
        );
        let declared = self
            .job
            .payload_files()
            .into_iter()
            .map(|f| (f.path.clone(), f))
            .collect::<BTreeMap<_, _>>();
        let mut input = Vec::new();
        let mut output = Vec::new();
        let mut aux = Vec::new();
        let mut exact = BTreeSet::new();
        let mut compatible = BTreeSet::new();
        let mut names = BTreeSet::new();
        let input_bound = self.job.inventory_bounds(false)?.1;
        let output_bound = self.job.inventory_bounds(true)?.1;
        for f in &self.files {
            f.validate()?;
            ensure!(
                exact.insert(path_collision_key(f.entry.path.as_str()))
                    && compatible.insert(compatibility_collision_key(f.entry.path.as_str()))
                    && names.insert(f.entry.path.clone()),
                "fetch_path_alias"
            );
            let bound = if declared.contains_key(&f.entry.path) {
                input_bound
            } else if f.entry.path.as_str().starts_with("out/") {
                output_bound
            } else if f.entry.path.as_str().starts_with("receipts/") {
                auxiliary.1
            } else {
                ensure!(
                    control_name(f.entry.path.as_str()),
                    "fetch_path_outside_job_classes"
                );
                (MAX_RECORD_BYTES as u64).min(auxiliary.1)
            };
            ensure!(f.entry.identity.bytes <= bound, "fetch_file_size");
            if declared.contains_key(&f.entry.path) {
                input.push(f.entry.clone());
            } else if f.entry.path.as_str().starts_with("out/") {
                output.push(f.entry.clone());
            } else {
                aux.push(f.entry.clone());
            }
        }
        self.job.verify_payload(&input)?;
        self.job.validate_inventory_capacity(&output, true)?;
        let auxiliary_bytes = aux.iter().try_fold(0u64, |sum, f| {
            sum.checked_add(f.identity.bytes)
                .ok_or_else(|| anyhow::anyhow!("fetch_auxiliary_size_overflow"))
        })?;
        ensure!(
            aux.len() <= auxiliary.2 && auxiliary_bytes <= auxiliary.0,
            "fetch_auxiliary_bound"
        );
        ensure!(
            self.files
                .iter()
                .any(|f| f.entry.path.as_str() == "job.json"
                    && f.entry.identity == self.binding.job_record),
            "fetch_job_record_missing"
        );
        let state = self
            .files
            .iter()
            .find(|f| f.entry.path.as_str() == "state.json")
            .map(|f| &f.entry.identity);
        ensure!(
            state == self.binding.state_record.as_ref(),
            "fetch_state_record_differs"
        );
        for d in &self.directories {
            let top = d.as_str().split('/').next().unwrap();
            ensure!(
                ["worker", "sources", "build", "inputs", "out", "receipts"].contains(&top)
                    && exact.insert(path_collision_key(d.as_str()))
                    && compatible.insert(compatibility_collision_key(d.as_str())),
                "fetch_directory_alias_or_scope"
            );
        }
        let directories = self
            .directories
            .iter()
            .map(|d| d.as_str())
            .collect::<BTreeSet<_>>();
        for path in self
            .files
            .iter()
            .map(|f| f.entry.path.as_str())
            .chain(self.directories.iter().map(|d| d.as_str()))
        {
            for (i, _) in path.match_indices('/') {
                ensure!(
                    directories.contains(&path[..i]),
                    "fetch_directory_parent_missing"
                );
            }
        }
        ensure!(
            serde_json::to_vec(self)?.len() <= MAX_SNAPSHOT_BYTES,
            "fetch_snapshot_size"
        );
        Ok(())
    }
    pub fn digest(&self) -> Result<Digest> {
        self.validate()?;
        record_digest(self)
    }
    pub fn range(&self, index: usize, offset: u64, bytes: u64) -> Result<FetchRange> {
        self.validate()?;
        let r = FetchRange {
            schema: "oev2.nx-fetch-range/1".into(),
            binding: self.binding.clone(),
            snapshot_digest: self.digest()?,
            file: self
                .files
                .get(index)
                .ok_or_else(|| anyhow::anyhow!("fetch_file_index"))?
                .clone(),
            offset,
            bytes,
        };
        r.validate()?;
        Ok(r)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        ensure!(bytes.len() <= MAX_SNAPSHOT_BYTES, "fetch_snapshot_size");
        let s: Self = serde_json::from_value(oe_wire::json::strict_json(bytes)?)?;
        s.validate()?;
        Ok(s)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FetchRange {
    pub schema: String,
    pub binding: FetchBinding,
    pub snapshot_digest: Digest,
    pub file: FetchFile,
    pub offset: u64,
    pub bytes: u64,
}
impl FetchRange {
    pub fn validate(&self) -> Result<()> {
        self.binding.validate()?;
        self.file.validate()?;
        ensure!(
            self.schema == "oev2.nx-fetch-range/1"
                && self.bytes > 0
                && self.bytes <= MAX_PAYLOAD_BYTES
                && self
                    .offset
                    .checked_add(self.bytes)
                    .is_some_and(|n| n <= self.file.entry.identity.bytes),
            "fetch_range_bound"
        );
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FetchSnapshotResponse {
    pub schema: String,
    pub binding: FetchBinding,
    pub snapshot_digest: Digest,
    pub payload: BlobIdentity,
}
impl FetchSnapshotResponse {
    pub fn decode_frame(
        frame: Frame,
        root: &RootIdentity,
        id: &JobId,
        digest: &Digest,
    ) -> Result<FetchSnapshot> {
        let response: Self = serde_json::from_value(frame.record.clone())?;
        ensure!(
            response.schema == "oev2.nx-fetch-snapshot-response/1"
                && response.binding.root == *root
                && response.binding.job_id == *id
                && response.binding.manifest_digest == *digest
                && response.payload.bytes <= MAX_SNAPSHOT_BYTES as u64,
            "fetch_snapshot_response_binding"
        );
        frame.validate_payload(Some(&response.payload))?;
        let snapshot = FetchSnapshot::decode(&frame.payload)?;
        ensure!(
            snapshot.binding == response.binding && snapshot.digest()? == response.snapshot_digest,
            "fetch_snapshot_response_digest"
        );
        Ok(snapshot)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FetchRangeResponse {
    pub schema: String,
    pub request_digest: Digest,
    pub payload: BlobIdentity,
    pub whole_file_hash_verified: bool,
}
impl FetchRangeResponse {
    pub fn validate(&self, request: &FetchRange, body: &[u8]) -> Result<()> {
        request.validate()?;
        ensure!(
            self.schema == "oev2.nx-fetch-range-response/1"
                && self.request_digest == record_digest(request)?
                && self.payload.bytes == request.bytes
                && !self.whole_file_hash_verified,
            "fetch_range_response_binding"
        );
        self.payload.verify(body)
    }
}

pub const MAX_FETCH_BATCH: usize = 512;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FetchSlice {
    pub file: FetchFile,
    pub offset: u64,
    pub bytes: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FetchBatch {
    pub schema: String,
    pub binding: FetchBinding,
    pub snapshot_digest: Digest,
    pub slices: Vec<FetchSlice>,
}
impl FetchBatch {
    pub fn validate(&self) -> Result<u64> {
        self.binding.validate()?;
        ensure!(
            self.schema == "oev2.nx-fetch-batch/1"
                && (1..=MAX_FETCH_BATCH).contains(&self.slices.len()),
            "fetch_batch_count"
        );
        let mut total = 0u64;
        for s in &self.slices {
            s.file.validate()?;
            ensure!(
                s.bytes > 0
                    && s.bytes <= MAX_PAYLOAD_BYTES
                    && s.offset
                        .checked_add(s.bytes)
                        .is_some_and(|n| n <= s.file.entry.identity.bytes),
                "fetch_batch_slice_bound"
            );
            total = total
                .checked_add(s.bytes)
                .ok_or_else(|| anyhow::anyhow!("fetch_batch_overflow"))?;
        }
        ensure!(
            total <= MAX_PAYLOAD_BYTES && serde_json::to_vec(self)?.len() <= MAX_RECORD_BYTES,
            "fetch_batch_bound"
        );
        Ok(total)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FetchBatchResponse {
    pub schema: String,
    pub request_digest: Digest,
    pub payload: BlobIdentity,
    pub whole_file_hash_verified: bool,
}
impl FetchBatchResponse {
    pub fn validate(&self, request: &FetchBatch, body: &[u8]) -> Result<()> {
        let total = request.validate()?;
        ensure!(
            self.schema == "oev2.nx-fetch-batch-response/1"
                && self.request_digest == record_digest(request)?
                && self.payload.bytes == total
                && !self.whole_file_hash_verified,
            "fetch_batch_response_binding"
        );
        self.payload.verify(body)
    }
}
/// Build bounded contiguous requests once. No per-file whole-snapshot rehash.
pub fn batches(snapshot: &FetchSnapshot, limit: u64) -> Result<Vec<FetchBatch>> {
    snapshot.validate()?;
    let limit = limit.min(
        snapshot
            .job
            .capacity
            .as_ref()
            .map_or(MAX_PAYLOAD_BYTES, |c| c.0.max_segment_bytes),
    );
    ensure!(
        (1..=MAX_PAYLOAD_BYTES).contains(&limit),
        "fetch_batch_limit"
    );
    let minimum = snapshot.files.iter().try_fold(0u64, |sum, f| {
        sum.checked_add(f.entry.identity.bytes.div_ceil(limit))
            .ok_or_else(|| anyhow::anyhow!("fetch_slice_overflow"))
    })?;
    ensure!(minimum <= 65536, "fetch_slice_count");
    let digest = snapshot.digest()?;
    let mut total_slices = 0usize;
    let mut batches = Vec::new();
    let mut slices = Vec::new();
    let mut used = 0;
    for file in &snapshot.files {
        let mut offset = 0;
        while offset < file.entry.identity.bytes {
            if used == limit || slices.len() == MAX_FETCH_BATCH {
                batches.push(FetchBatch {
                    schema: "oev2.nx-fetch-batch/1".into(),
                    binding: snapshot.binding.clone(),
                    snapshot_digest: digest.clone(),
                    slices: std::mem::take(&mut slices),
                });
                used = 0;
            }
            let bytes = (file.entry.identity.bytes - offset).min(limit - used);
            total_slices += 1;
            ensure!(total_slices <= 65536, "fetch_slice_count");
            slices.push(FetchSlice {
                file: file.clone(),
                offset,
                bytes,
            });
            offset += bytes;
            used += bytes;
        }
    }
    if !slices.is_empty() {
        batches.push(FetchBatch {
            schema: "oev2.nx-fetch-batch/1".into(),
            binding: snapshot.binding.clone(),
            snapshot_digest: digest,
            slices,
        });
    }
    for b in &batches {
        b.validate()?;
    }
    Ok(batches)
}
