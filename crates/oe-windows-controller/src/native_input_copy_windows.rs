//! Source pins survive the whole copy; the caller supplies both held root locks.
use crate::{
    adapter,
    native_input_copy::{Destination, Source},
    transfer_publication::{self, Operation},
    transfer_reconstruct::Target,
    transfer_reconstruct_windows::PinnedTarget,
    windows::*,
};
use anyhow::{Result, ensure};
use oe_nx_protocol::{self as p, segmented::TransferPlan, transfer::SourcePublication};
use std::io::Write;
pub struct PublishedSource<'a, 'root> {
    tree: PinnedTarget<'a, 'root>,
    publication: SourcePublication,
    plan: &'a TransferPlan,
    metadata: &'a PinnedDirectory<'root>,
    pins: Vec<PinnedFile>,
}
impl<'a, 'root> PublishedSource<'a, 'root> {
    pub fn open(
        root: &StableRoot,
        lock: &'a WriterLock<'root>,
        output: &'a PinnedDirectory<'root>,
        metadata: &'a PinnedDirectory<'root>,
        plan: &'a TransferPlan,
        expected: &SourcePublication,
    ) -> Result<Self> {
        ensure!(
            adapter::root_identity(root) == plan.root,
            "native_input_source_root"
        );
        let current = transfer_publication::publish(
            root,
            lock,
            plan,
            &expected.output_id,
            &expected.publication_id,
            &expected.target,
            Operation::Inspect,
        )?;
        ensure!(&current == expected, "native_input_publication_changed");
        let mut tree = PinnedTarget::new(output, lock)?;
        ensure!(
            tree.identity()? == expected.target,
            "native_input_source_tree_changed"
        );
        let mut metadata_id = adapter::root_identity(root);
        metadata_id.volume_id = metadata.identity().volume_id.clone();
        metadata_id.directory_id = metadata.identity().file_id.clone();
        ensure!(
            metadata_id == expected.publication_root,
            "native_input_publication_directory_changed"
        );
        let raw = serde_json::to_vec(expected)?;
        let (state, pending, pins) = transfer_publication::records(metadata, &raw)?;
        ensure!(state && !pending, "native_input_publication_not_committed");
        for f in &plan.files {
            ensure!(
                tree.readback(f)? == f.identity,
                "native_input_source_changed"
            );
        }
        let result = Self {
            tree,
            publication: current,
            plan,
            metadata,
            pins,
        };
        result.verify()?;
        Ok(result)
    }
}
impl Source for PublishedSource<'_, '_> {
    fn publication(&self) -> &SourcePublication {
        &self.publication
    }
    fn files(&self) -> &[p::FileEntry] {
        &self.plan.files
    }
    fn verify(&self) -> Result<()> {
        ensure!(
            self.plan.binding()? == self.publication.plan_digest,
            "native_input_plan_changed"
        );
        self.tree.verify_complete(&self.plan.files)?;
        for pin in &self.pins {
            pin.verify()?;
        }
        let (state, pending, _) =
            transfer_publication::records(self.metadata, &serde_json::to_vec(&self.publication)?)?;
        ensure!(state && !pending, "native_input_publication_pending");
        Ok(())
    }
    fn stream(&mut self, file: &p::FileEntry, max_segment: u64, out: &mut dyn Write) -> Result<()> {
        self.tree.stream_pinned(file, max_segment, out)
    }
}
impl Destination for PinnedTarget<'_, '_> {
    fn root_identity(&self) -> Result<p::RootIdentity> {
        self.owner_root()
    }
}
