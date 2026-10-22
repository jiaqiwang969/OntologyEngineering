//! Full publication-to-job mapping, before any native dispatch authority.
use crate::{transfer::SourcePublication, *};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mount {
    Inputs,
    Payload,
}
impl Mount {
    pub fn map(&self, f: &FileEntry) -> Result<FileEntry> {
        Ok(FileEntry {
            path: RelativePath::new(match self {
                Self::Inputs => format!("inputs/{}", f.path.as_str()),
                Self::Payload => f.path.as_str().into(),
            })?,
            identity: f.identity.clone(),
        })
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublishedPayloadSource {
    pub publication: SourcePublication,
    pub files: Vec<FileEntry>,
    pub mount: Mount,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublishedPayloadPlan {
    pub schema: String,
    pub job_manifest_digest: Digest,
    pub sources: Vec<PublishedPayloadSource>,
}
impl PublishedPayloadPlan {
    pub fn validate(&self, job: &JobManifest) -> Result<()> {
        job.validate()?;
        ensure!(
            job.schema == LARGE_JOB_SCHEMA
                && self.schema == "oev2.nx-published-payload-plan/1"
                && self.job_manifest_digest == job.digest()?,
            "published_payload_job_binding"
        );
        ensure!(
            (1..=16).contains(&self.sources.len()),
            "published_payload_source_count"
        );
        let mut mapped = Vec::new();
        for source in &self.sources {
            let p = &source.publication;
            let files = &source.files;
            p.source_root.validate()?;
            p.target.validate()?;
            p.publication_root.validate()?;
            ensure!(
                p.schema == "oev2.nx-source-publication/1"
                    && p.state == "published"
                    && !p.native_execution_started,
                "published_payload_uncommitted_source"
            );
            ensure!(
                p.job_id != p.output_id
                    && p.job_id != p.publication_id
                    && p.output_id != p.publication_id,
                "published_payload_source_names_alias"
            );
            ensure!(
                p.source_root != p.target
                    && p.source_root != p.publication_root
                    && p.target != p.publication_root,
                "published_payload_source_ids_alias"
            );
            for root in [&p.source_root, &p.target, &p.publication_root] {
                ensure!(
                    *root != job.root
                        && root.host_id == job.root.host_id
                        && root.volume_id == job.root.volume_id,
                    "published_payload_root_binding"
                );
            }
            ensure!(
                !files.is_empty() && files.windows(2).all(|w| w[0].path < w[1].path),
                "published_payload_source_order"
            );
            let total = files.iter().try_fold(0u64, |n, f| {
                n.checked_add(f.identity.bytes)
                    .ok_or_else(|| anyhow::anyhow!("published_payload_overflow"))
            })?;
            ensure!(
                p.files == files.len()
                    && p.bytes == total
                    && p.inventory_digest == record_digest(files)?,
                "published_payload_inventory_binding"
            );
            for file in files {
                mapped.push(source.mount.map(file)?);
            }
        }
        job.verify_payload(&mapped)?;
        ensure!(
            serde_json::to_vec(self)?.len() <= MAX_RECORD_BYTES,
            "published_payload_plan_record_bound"
        );
        Ok(())
    }
    pub fn digest(&self, job: &JobManifest) -> Result<Digest> {
        self.validate(job)?;
        record_digest(self)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedPayload {
    pub schema: String,
    pub job_manifest_digest: Digest,
    pub source_plan_digest: Digest,
    pub target: RootIdentity,
    pub payload_digest: Digest,
    pub files: usize,
    pub bytes: u64,
    pub job_staged: bool,
    pub native_execution_started: bool,
}
impl PreparedPayload {
    pub fn validate(&self, job: &JobManifest, plan: &PublishedPayloadPlan) -> Result<()> {
        plan.validate(job)?;
        self.target.validate()?;
        let mut files = job.payload_files();
        files.sort_by(|a, b| a.path.cmp(&b.path));
        ensure!(
            self.schema == "oev2.nx-prepared-payload/1"
                && self.job_manifest_digest == job.digest()?
                && self.source_plan_digest == plan.digest(job)?
                && self.payload_digest == record_digest(&files)?
                && self.files == files.len()
                && self.bytes == files.iter().map(|f| f.identity.bytes).sum::<u64>()
                && !self.job_staged
                && !self.native_execution_started,
            "prepared_payload_receipt_binding"
        );
        ensure!(
            self.target.host_id == job.root.host_id
                && self.target.volume_id == job.root.volume_id
                && self.target != job.root,
            "prepared_payload_target_binding"
        );
        for s in &plan.sources {
            ensure!(
                self.target != s.publication.target
                    && self.target != s.publication.source_root
                    && self.target != s.publication.publication_root,
                "prepared_payload_target_alias"
            );
        }
        Ok(())
    }
}
