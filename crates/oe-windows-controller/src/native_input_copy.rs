//! Full published input preparation, not complete job staging or execution.
use crate::transfer_reconstruct::Target;
use anyhow::{Result, ensure};
use oe_nx_protocol::{transfer::SourcePublication, *};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::io::Write;
pub trait Source {
    fn publication(&self) -> &SourcePublication;
    fn files(&self) -> &[FileEntry];
    fn verify(&self) -> Result<()>;
    fn stream(&mut self, file: &FileEntry, max_segment: u64, out: &mut dyn Write) -> Result<()>;
}
pub trait Destination: Target {
    fn root_identity(&self) -> Result<RootIdentity>;
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedInputs {
    pub schema: String,
    pub job_manifest_digest: Digest,
    pub publication_digest: Digest,
    pub target: RootIdentity,
    pub inventory_digest: Digest,
    pub files: usize,
    pub bytes: u64,
    pub job_staged: bool,
    pub native_execution_started: bool,
}
struct CheckedWriter<'a> {
    out: &'a mut dyn Write,
    hash: Sha256,
    bytes: u64,
    expected: u64,
    max_segment: u64,
}
impl Write for CheckedWriter<'_> {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        if b.len() as u64 > self.max_segment
            || self
                .bytes
                .checked_add(b.len() as u64)
                .is_none_or(|n| n > self.expected)
        {
            return Err(std::io::Error::other("native_input_stream_bound"));
        }
        let n = self.out.write(b)?;
        if n > b.len() {
            return Err(std::io::Error::other("invalid_write_count"));
        }
        self.hash.update(&b[..n]);
        self.bytes += n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.out.flush()
    }
}
pub fn prepare<S: Source, T: Destination>(
    job: &JobManifest,
    source: &mut S,
    target: &mut T,
) -> Result<PreparedInputs> {
    job.validate()?;
    ensure!(
        job.schema == LARGE_JOB_SCHEMA,
        "native_input_copy_requires_v3"
    );
    let p = source.publication().clone();
    let files = source.files().to_vec();
    ensure!(
        p.schema == "oev2.nx-source-publication/1"
            && p.state == "published"
            && !p.native_execution_started,
        "native_input_source_not_published"
    );
    p.source_root.validate()?;
    p.target.validate()?;
    p.publication_root.validate()?;
    ensure!(
        p.source_root != job.root && p.target != job.root && p.publication_root != job.root,
        "native_input_source_destination_alias"
    );
    ensure!(
        p.source_root.host_id == job.root.host_id && p.source_root.volume_id == job.root.volume_id,
        "native_input_cross_host_or_volume"
    );
    ensure!(
        p.files == files.len() && p.inventory_digest == record_digest(&files)?,
        "native_input_publication_inventory"
    );
    let bytes = files.iter().try_fold(0u64, |n, f| {
        n.checked_add(f.identity.bytes)
            .ok_or_else(|| anyhow::anyhow!("native_input_total_overflow"))
    })?;
    ensure!(p.bytes == bytes, "native_input_publication_bytes");
    let mut mapped = Vec::new();
    for f in &files {
        mapped.push(FileEntry {
            path: RelativePath::new(format!("inputs/{}", f.path.as_str()))?,
            identity: f.identity.clone(),
        });
    }
    let mut expected = job.inputs.clone();
    mapped.sort_by(|a, b| a.path.cmp(&b.path));
    expected.sort_by(|a, b| a.path.cmp(&b.path));
    ensure!(mapped == expected, "native_input_job_mapping_differs");
    ensure!(
        target.root_identity()? == job.root,
        "native_input_destination_root_differs"
    );
    let identity = target.identity()?;
    identity.validate()?;
    ensure!(
        identity.host_id == job.root.host_id
            && identity.volume_id == job.root.volume_id
            && identity != job.root
            && identity != p.target
            && identity != p.publication_root
            && identity != p.source_root,
        "native_input_destination_identity"
    );
    source.verify()?;
    target.verify_empty()?;
    let max_segment = job.capacity.as_ref().unwrap().0.max_segment_bytes;
    for file in &files {
        let output = FileEntry {
            path: RelativePath::new(format!("inputs/{}", file.path.as_str()))?,
            identity: file.identity.clone(),
        };
        target.write_new(&output, |writer| {
            stream_checked(source, file, max_segment, writer)
        })?;
        ensure!(
            target.readback(&output)? == output.identity,
            "native_input_readback_differs"
        );
    }
    source.verify()?;
    target.verify_complete(&mapped)?;
    ensure!(
        target.identity()? == identity && target.root_identity()? == job.root,
        "native_input_destination_changed"
    );
    Ok(PreparedInputs {
        schema: "oev2.nx-prepared-inputs/1".into(),
        job_manifest_digest: job.digest()?,
        publication_digest: record_digest(&p)?,
        target: identity,
        inventory_digest: record_digest(&mapped)?,
        files: files.len(),
        bytes,
        job_staged: false,
        native_execution_started: false,
    })
}

pub(crate) fn stream_checked<S: Source + ?Sized>(
    source: &mut S,
    file: &FileEntry,
    max_segment: u64,
    out: &mut dyn Write,
) -> Result<()> {
    let mut writer = CheckedWriter {
        out,
        hash: Sha256::new(),
        bytes: 0,
        expected: file.identity.bytes,
        max_segment,
    };
    source.stream(file, max_segment, &mut writer)?;
    writer.flush()?;
    ensure!(
        writer.bytes == file.identity.bytes
            && Digest::new(format!("{:x}", writer.hash.finalize()))? == file.identity.sha256,
        "native_input_copy_content_differs"
    );
    Ok(())
}
