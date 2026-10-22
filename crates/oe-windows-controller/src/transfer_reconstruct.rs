//! Stream an already-complete transfer into a fresh, unpublished tree.
//! A successful receipt is neither atomic publication nor native job admission.
use crate::transfer_store::{Session, Store};
use anyhow::{Result, ensure};
use oe_nx_protocol::{
    BlobIdentity, FileEntry, MAX_RECORD_BYTES, RootIdentity, decode, record_digest,
    segmented::{Receiver, TransferPlan},
};
use std::io::Write;
pub const MAX_RECONSTRUCTED_FILE_BYTES: u64 = 8 * 1024 * 1024 * 1024;
pub trait Target {
    fn identity(&self) -> Result<RootIdentity>;
    fn verify_empty(&self) -> Result<()>;
    /// Exclusive creation, bounded stream, durable flush. Failure retains partial
    /// bytes; the same destination cannot be retried or silently overwritten.
    fn write_new<F: FnOnce(&mut dyn Write) -> Result<()>>(
        &mut self,
        file: &FileEntry,
        write: F,
    ) -> Result<()>;
    /// Independently reopen and hash the whole output; retain immutable handles.
    fn readback(&mut self, file: &FileEntry) -> Result<BlobIdentity>;
    /// Exact namespace plus all retained identities, before returning any receipt.
    fn verify_complete(&self, files: &[FileEntry]) -> Result<()>;
}
pub use oe_nx_protocol::transfer::ReconstructedTree;
pub fn validate_plan(plan: &TransferPlan) -> Result<()> {
    plan.validate()?;
    ensure!(
        plan.files
            .iter()
            .all(|f| f.identity.bytes <= MAX_RECONSTRUCTED_FILE_BYTES),
        "reconstruction_file_bound"
    );
    Ok(())
}
pub fn reconstruct<S: Store, T: Target>(
    source: &mut S,
    plan: TransferPlan,
    target: &mut T,
) -> Result<ReconstructedTree> {
    validate_plan(&plan)?;
    let binding = plan.binding()?;
    // Current actual chunks/receipts must be complete before touching destination.
    Session::open_existing(source, plan.clone())?.finish()?;
    let identity = target.identity()?;
    identity.validate()?;
    ensure!(
        identity != plan.root,
        "reconstruction_target_is_source_root"
    );
    target.verify_empty()?;
    let mut receiver = Receiver::new(plan.clone())?;
    let mut index = 0;
    for (file_index, file) in plan.files.iter().enumerate() {
        target.write_new(file, |writer| {
            while let Some(span) = plan
                .spans
                .get(index)
                .filter(|s| s.file_index as usize == file_index)
            {
                let raw = source.read(
                    &format!("chunk-{index:05}.bin"),
                    plan.budget.max_chunk_bytes,
                )?;
                ensure!(
                    raw.len() as u64 == span.bytes,
                    "reconstruction_chunk_length"
                );
                receiver.accept(&binding, index, &raw)?;
                writer.write_all(&raw)?;
                index += 1;
            }
            Ok(())
        })?;
        ensure!(
            target.readback(file)? == file.identity,
            "reconstruction_readback_differs"
        );
    }
    let coverage = receiver.finish()?;
    ensure!(
        coverage.files == plan.files.len(),
        "reconstruction_coverage"
    );
    // Retain the original plan identity and check source handles again after copy.
    let stored: TransferPlan = decode(&source.read("plan.json", MAX_RECORD_BYTES as u64)?)?;
    ensure!(stored == plan, "reconstruction_source_plan_changed");
    source.verify()?;
    target.verify_complete(&plan.files)?;
    ensure!(
        target.identity()? == identity,
        "reconstruction_target_identity_changed"
    );
    Ok(ReconstructedTree {
        schema: "oev2.nx-reconstructed-tree/1".into(),
        plan_digest: binding,
        target: identity,
        inventory_digest: record_digest(&plan.files)?,
        files: coverage.files,
        bytes: coverage.bytes,
        published: false,
        native_execution_started: false,
    })
}
