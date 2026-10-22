//! Explicit large-task byte coverage. No file writes, network, dispatch, or
//! durable receipt is implied by this in-memory verifier.
use crate::{
    Digest, FileEntry, JobId, MAX_FILES, MAX_PAYLOAD_BYTES, MAX_RECORD_BYTES, RootIdentity,
};
use anyhow::{Context, Result, ensure};
use oe_contracts::NxTransferBudget;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Span {
    pub file_index: u32,
    pub offset: u64,
    pub bytes: u64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransferPlan {
    pub schema: String,
    pub job_id: JobId,
    pub root: RootIdentity,
    pub direction: String,
    pub source_manifest: Digest,
    pub budget: NxTransferBudget,
    pub files: Vec<FileEntry>,
    pub spans: Vec<Span>,
}
fn limits(b: &NxTransferBudget) -> Result<()> {
    b.validate()
        .map_err(|_| anyhow::anyhow!("transfer_generated_budget_invalid"))?;
    ensure!(
        b.max_total_bytes > 0
            && b.max_total_bytes <= 64 * 1024 * 1024 * 1024
            && b.max_chunk_bytes > 0
            && b.max_chunk_bytes <= MAX_PAYLOAD_BYTES
            && b.max_files > 0
            && b.max_files <= MAX_FILES as u64
            && b.max_chunks > 0
            && b.max_chunks <= 65536,
        "transfer_budget_invalid"
    );
    Ok(())
}
fn spans(files: &[FileEntry], b: &NxTransferBudget) -> Result<Vec<Span>> {
    limits(b)?;
    ensure!(
        !files.is_empty() && files.len() as u64 <= b.max_files,
        "transfer_file_budget"
    );
    crate::identity::validate_inventory_with_limit(files, b.max_total_bytes)?;
    ensure!(
        files.windows(2).all(|w| w[0].path < w[1].path),
        "transfer_inventory_not_canonical"
    );
    let mut result = Vec::new();
    for (i, f) in files.iter().enumerate() {
        let mut offset = 0;
        loop {
            ensure!(
                (result.len() as u64) < b.max_chunks,
                "transfer_chunk_budget"
            );
            let bytes = (f.identity.bytes - offset).min(b.max_chunk_bytes);
            result.push(Span {
                file_index: i as u32,
                offset,
                bytes,
            });
            offset += bytes;
            if offset == f.identity.bytes {
                break;
            }
        }
    }
    Ok(result)
}
impl TransferPlan {
    pub fn new(
        job_id: JobId,
        root: RootIdentity,
        direction: String,
        source_manifest: Digest,
        budget: NxTransferBudget,
        mut files: Vec<FileEntry>,
    ) -> Result<Self> {
        files.sort_by(|a, b| a.path.cmp(&b.path));
        let chunks = spans(&files, &budget)?;
        let p = Self {
            schema: "oev2.nx-segmented-transfer/1".into(),
            job_id,
            root,
            direction,
            source_manifest,
            budget,
            files,
            spans: chunks,
        };
        p.validate()?;
        Ok(p)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == "oev2.nx-segmented-transfer/1",
            "transfer_schema"
        );
        self.root.validate()?;
        ensure!(
            matches!(self.direction.as_str(), "upload" | "download"),
            "transfer_direction"
        );
        ensure!(
            self.spans == spans(&self.files, &self.budget)?,
            "transfer_coverage_differs"
        );
        ensure!(
            serde_json::to_vec(self)?.len() <= MAX_RECORD_BYTES,
            "transfer_manifest_record_budget"
        );
        Ok(())
    }
    pub fn binding(&self) -> Result<Digest> {
        self.validate()?;
        crate::record_digest(self)
    }
}
/// A receiver instance is terminal after any mismatch. An external durable
/// receiver must additionally persist intents, writes and re-read whole files.
#[derive(Debug, PartialEq, Eq)]
pub struct VerifiedCoverage {
    pub plan_digest: Digest,
    pub files: usize,
    pub bytes: u64,
}
pub struct Receiver {
    plan: TransferPlan,
    binding: Digest,
    next: usize,
    hash: Sha256,
    poisoned: bool,
}
impl Receiver {
    pub fn new(plan: TransferPlan) -> Result<Self> {
        let binding = plan.binding()?;
        Ok(Self {
            plan,
            binding,
            next: 0,
            hash: Sha256::new(),
            poisoned: false,
        })
    }
    pub fn accept(&mut self, binding: &Digest, index: usize, bytes: &[u8]) -> Result<()> {
        ensure!(!self.poisoned, "transfer_receiver_terminal");
        let r = self.accept_inner(binding, index, bytes);
        if r.is_err() {
            self.poisoned = true;
        }
        r
    }
    fn accept_inner(&mut self, binding: &Digest, index: usize, bytes: &[u8]) -> Result<()> {
        ensure!(
            binding == &self.binding && index == self.next,
            "transfer_binding_or_order"
        );
        let span = self
            .plan
            .spans
            .get(index)
            .context("transfer_unexpected_chunk")?;
        ensure!(bytes.len() as u64 == span.bytes, "transfer_chunk_length");
        self.hash.update(bytes);
        let file = &self.plan.files[span.file_index as usize];
        if span.offset + span.bytes == file.identity.bytes {
            let got = Digest::new(format!("{:x}", self.hash.clone().finalize()))?;
            ensure!(got == file.identity.sha256, "transfer_file_digest_differs");
            self.hash = Sha256::new();
        }
        self.next += 1;
        Ok(())
    }
    pub fn finish(mut self) -> Result<VerifiedCoverage> {
        ensure!(
            !self.poisoned && self.next == self.plan.spans.len(),
            "transfer_incomplete_or_failed"
        );
        self.poisoned = true;
        // Identity of the verified plan, NOT a durable on-disk or native result.
        Ok(VerifiedCoverage {
            plan_digest: self.binding,
            files: self.plan.files.len(),
            bytes: self.plan.files.iter().map(|f| f.identity.bytes).sum(),
        })
    }
}
