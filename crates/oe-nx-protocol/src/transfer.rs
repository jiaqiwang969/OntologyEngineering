//! Transfer-only operations reuse bounded frames, never v2 native job admission.
use crate::*;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
pub const RESPONSE_SCHEMA: &str = "oev2.nx-transfer-response/1";
pub fn is_transfer(r: &Request) -> bool {
    matches!(
        r,
        Request::TransferBegin { .. }
            | Request::TransferPublish { .. }
            | Request::TransferInspectPublication { .. }
            | Request::TransferRecoverPublication { .. }
            | Request::TransferReconstruct { .. }
            | Request::TransferPut { .. }
            | Request::TransferPutBatch { .. }
            | Request::TransferInspect { .. }
            | Request::TransferCommit { .. }
            | Request::TransferVerify { .. }
    )
}
pub fn plan(record: &str) -> Result<segmented::TransferPlan> {
    let p: segmented::TransferPlan = decode(record.as_bytes())?;
    p.validate()?;
    ensure!(
        p.direction == "upload",
        "transfer_direction_not_supported_by_endpoint"
    );
    Ok(p)
}
pub fn binding(r: &Request) -> Result<(JobId, Digest)> {
    match r {
        Request::TransferBegin { plan_record } => {
            let p = plan(plan_record)?;
            Ok((p.job_id.clone(), p.binding()?))
        }
        Request::TransferPublish {
            job_id,
            plan_digest,
            ..
        }
        | Request::TransferInspectPublication {
            job_id,
            plan_digest,
            ..
        }
        | Request::TransferRecoverPublication {
            job_id,
            plan_digest,
            ..
        }
        | Request::TransferReconstruct {
            job_id,
            plan_digest,
            ..
        }
        | Request::TransferPut {
            job_id,
            plan_digest,
            ..
        }
        | Request::TransferPutBatch {
            job_id,
            plan_digest,
            ..
        }
        | Request::TransferInspect {
            job_id,
            plan_digest,
        }
        | Request::TransferCommit {
            job_id,
            plan_digest,
        }
        | Request::TransferVerify {
            job_id,
            plan_digest,
        } => Ok((job_id.clone(), plan_digest.clone())),
        _ => anyhow::bail!("not_transfer_request"),
    }
}
pub fn validate_request(r: &Request) -> Result<()> {
    binding(r)?;
    if let Request::TransferPublish {
        job_id,
        output_id,
        publication_id,
        expected_target,
        ..
    }
    | Request::TransferInspectPublication {
        job_id,
        output_id,
        publication_id,
        expected_target,
        ..
    }
    | Request::TransferRecoverPublication {
        job_id,
        output_id,
        publication_id,
        expected_target,
        ..
    } = r
    {
        expected_target.validate()?;
        ensure!(
            job_id != output_id && publication_id != output_id && publication_id != job_id,
            "publication_namespace_alias"
        );
    }
    if let Request::TransferReconstruct {
        job_id, output_id, ..
    } = r
    {
        ensure!(job_id != output_id, "reconstruction_output_is_input");
    }
    if let Request::TransferPutBatch {
        index,
        count,
        payload,
        ..
    } = r
    {
        ensure!(
            *count > 0
                && *count <= 4096
                && index.checked_add(*count).is_some_and(|end| end <= 65536)
                && payload.bytes <= MAX_PAYLOAD_BYTES,
            "transfer_batch_request_bound"
        );
    }
    if let Request::TransferPut { index, payload, .. } = r {
        ensure!(
            *index < 65536 && payload.bytes <= MAX_PAYLOAD_BYTES,
            "transfer_request_bound"
        );
    }
    Ok(())
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourcePublication {
    pub schema: String,
    pub source_root: RootIdentity,
    pub job_id: JobId,
    pub plan_digest: Digest,
    pub output_id: JobId,
    pub publication_id: JobId,
    pub publication_root: RootIdentity,
    pub target: RootIdentity,
    pub inventory_digest: Digest,
    pub files: usize,
    pub bytes: u64,
    pub state: String,
    pub native_execution_started: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReconstructedTree {
    pub schema: String,
    pub plan_digest: Digest,
    pub target: RootIdentity,
    pub inventory_digest: Digest,
    pub files: usize,
    pub bytes: u64,
    pub published: bool,
    pub native_execution_started: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransferResponse {
    pub schema: String,
    pub root: RootIdentity,
    pub job_id: JobId,
    pub plan_digest: Digest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publication: Option<SourcePublication>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reconstructed: Option<ReconstructedTree>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reconstruction_output: Option<JobId>,
    pub next_index: u32,
    pub total_chunks: u32,
    pub status: String,
    pub verified_bytes: Option<u64>,
    pub native_execution_started: bool,
}
impl TransferResponse {
    pub fn validate(&self, request: &Request, root: &RootIdentity) -> Result<()> {
        validate_request(request)?;
        self.root.validate()?;
        let (job, digest) = binding(request)?;
        ensure!(
            self.schema == RESPONSE_SCHEMA
                && self.root == *root
                && self.job_id == job
                && self.plan_digest == digest
                && !self.native_execution_started,
            "transfer_response_binding"
        );
        ensure!(
            self.total_chunks > 0
                && self.total_chunks <= 65536
                && self.next_index <= self.total_chunks,
            "transfer_response_coverage"
        );
        if let Request::TransferPublish {
            output_id,
            publication_id,
            expected_target,
            ..
        }
        | Request::TransferInspectPublication {
            output_id,
            publication_id,
            expected_target,
            ..
        }
        | Request::TransferRecoverPublication {
            output_id,
            publication_id,
            expected_target,
            ..
        } = request
        {
            let p = self
                .publication
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("publication_receipt_missing"))?;
            p.publication_root.validate()?;
            p.target.validate()?;
            ensure!(
                p.schema == "oev2.nx-source-publication/1"
                    && p.source_root == *root
                    && p.job_id == job
                    && p.plan_digest == digest
                    && p.output_id == *output_id
                    && p.publication_id == *publication_id
                    && p.target == *expected_target,
                "publication_receipt_binding"
            );
            ensure!(
                p.publication_root.host_id == root.host_id
                    && p.publication_root.volume_id == root.volume_id
                    && p.publication_root.directory_id != root.directory_id
                    && p.publication_root != p.target
                    && p.target.host_id == root.host_id
                    && p.target.volume_id == root.volume_id
                    && p.target.directory_id != root.directory_id,
                "publication_physical_binding"
            );
            ensure!(
                p.state == "published"
                    && !p.native_execution_started
                    && p.files > 0
                    && p.files <= MAX_FILES
                    && p.bytes <= 64 * 1024 * 1024 * 1024
                    && self.status == "published_source"
                    && self.next_index == self.total_chunks
                    && self.verified_bytes == Some(p.bytes)
                    && self.reconstructed.is_none()
                    && self.reconstruction_output.is_none(),
                "publication_receipt_scope"
            );
            return Ok(());
        }
        ensure!(self.publication.is_none(), "unexpected_publication_receipt");
        if let Request::TransferReconstruct { output_id, .. } = request {
            let tree = self
                .reconstructed
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("reconstruction_receipt_missing"))?;
            tree.target.validate()?;
            ensure!(
                self.reconstruction_output.as_ref() == Some(output_id)
                    && tree.schema == "oev2.nx-reconstructed-tree/1"
                    && tree.plan_digest == digest
                    && tree.target.host_id == root.host_id
                    && tree.target.volume_id == root.volume_id
                    && tree.target.directory_id != root.directory_id
                    && !tree.published
                    && !tree.native_execution_started,
                "reconstruction_receipt_binding"
            );
            ensure!(
                tree.files > 0
                    && tree.files <= MAX_FILES
                    && tree.bytes <= 64 * 1024 * 1024 * 1024
                    && self.verified_bytes == Some(tree.bytes)
                    && self.status == "reconstructed_files"
                    && self.next_index == self.total_chunks,
                "reconstruction_receipt_coverage"
            );
            return Ok(());
        }
        ensure!(
            self.reconstructed.is_none() && self.reconstruction_output.is_none(),
            "unexpected_reconstruction_receipt"
        );
        let verified = matches!(request, Request::TransferVerify { .. });
        ensure!(
            self.status == if verified { "verified_chunks" } else { "ready" },
            "transfer_response_status"
        );
        ensure!(
            if verified {
                self.next_index == self.total_chunks
                    && self
                        .verified_bytes
                        .is_some_and(|b| b <= 64 * 1024 * 1024 * 1024)
            } else {
                self.verified_bytes.is_none()
            },
            "transfer_response_not_full_native_admission"
        );
        match request {
            Request::TransferBegin { plan_record } => {
                let p = plan(plan_record)?;
                ensure!(
                    p.root == *root
                        && self.next_index == 0
                        && self.total_chunks as usize == p.spans.len(),
                    "transfer_begin_response"
                );
            }
            Request::TransferPutBatch { index, count, .. } => {
                ensure!(self.next_index == index + count, "transfer_batch_response")
            }
            Request::TransferPut { index, .. } => {
                ensure!(self.next_index == index + 1, "transfer_put_response")
            }
            _ => {}
        }
        Ok(())
    }
}
