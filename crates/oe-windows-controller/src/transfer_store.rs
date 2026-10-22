//! Durable chunk orchestration shared with the Windows pinned-directory adapter.
//! Store implementers must retain an exclusive writer lock and stable root;
//! successful chunk storage is not job admission or native execution.
use anyhow::{Context, Result, ensure};
use oe_nx_protocol::{
    BlobIdentity, Digest, MAX_RECORD_BYTES, RootIdentity, decode,
    segmented::{Receiver, TransferPlan, VerifiedCoverage},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
pub trait Store {
    fn root(&self) -> Result<RootIdentity>;
    fn names(&self, limit: usize) -> Result<Vec<String>>;
    fn read(&mut self, name: &str, limit: u64) -> Result<Vec<u8>>;
    /// Exclusive create, complete write and durable flush; never overwrite.
    fn create_sync(&mut self, name: &str, bytes: &[u8]) -> Result<()>;
    /// Verify root, writer lock and directory before each storage operation.
    /// Default preserves the full check for adapters without a narrower proof.
    fn verify_scope(&self) -> Result<()> {
        self.verify()
    }
    fn verify(&self) -> Result<()>;
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChunkRecord {
    schema: String,
    binding: Digest,
    index: usize,
    payload: BlobIdentity,
}
fn name(index: usize, kind: &str) -> String {
    format!("chunk-{index:05}.{kind}")
}
fn bytes<T: Serialize>(x: &T) -> Result<Vec<u8>> {
    let b = serde_json::to_vec(x)?;
    ensure!(b.len() <= MAX_RECORD_BYTES, "transfer_store_record_budget");
    Ok(b)
}
fn root<S: Store>(s: &S, p: &TransferPlan) -> Result<()> {
    p.validate()?;
    s.verify()?;
    ensure!(s.root()? == p.root, "transfer_store_root_differs");
    Ok(())
}
/// explicit_commit permits only adding a missing receipt for a fully present,
/// verified chunk. Partial files remain held and are never overwritten.
pub struct Session<'a, S: Store> {
    store: &'a mut S,
    plan: TransferPlan,
    binding: Digest,
    receiver: Option<Receiver>,
    next: usize,
    valid: bool,
}
impl<'a, S: Store> Session<'a, S> {
    pub fn open(store: &'a mut S, plan: TransferPlan, explicit_commit: bool) -> Result<Self> {
        Self::open_mode(store, plan, explicit_commit, true)
    }
    pub fn open_existing(store: &'a mut S, plan: TransferPlan) -> Result<Self> {
        Self::open_mode(store, plan, false, false)
    }
    fn open_mode(
        store: &'a mut S,
        plan: TransferPlan,
        explicit_commit: bool,
        allow_create: bool,
    ) -> Result<Self> {
        root(store, &plan)?;
        let binding = plan.binding()?;
        let listed = store.names(1 + 3 * plan.spans.len())?;
        let mut names: BTreeSet<_> = listed.iter().cloned().collect();
        ensure!(
            names.len() == listed.len(),
            "transfer_store_duplicate_names"
        );
        if names.is_empty() {
            ensure!(allow_create, "transfer_existing_store_required");
            let raw = bytes(&plan)?;
            store.create_sync("plan.json", &raw)?;
            ensure!(
                store.read("plan.json", MAX_RECORD_BYTES as u64)? == raw,
                "transfer_store_plan_readback"
            );
        } else {
            ensure!(names.remove("plan.json"), "transfer_store_plan_missing");
            let old: TransferPlan = decode(&store.read("plan.json", MAX_RECORD_BYTES as u64)?)?;
            ensure!(old == plan, "transfer_store_plan_differs");
        }
        // Validate the entire name layout before an explicit recovery can write.
        let mut layout = names.clone();
        for i in 0..plan.spans.len() {
            if !layout.remove(&name(i, "intent.json")) {
                break;
            }
            let chunk = layout.remove(&name(i, "bin"));
            let receipt = layout.remove(&name(i, "receipt.json"));
            ensure!(!receipt || chunk, "transfer_store_receipt_without_chunk");
            if !chunk || !receipt {
                break;
            }
        }
        ensure!(
            layout.is_empty(),
            "transfer_store_unexpected_or_gapped_files"
        );
        let mut receiver = Receiver::new(plan.clone())?;
        let mut next = 0;
        for i in 0..plan.spans.len() {
            let intent_name = name(i, "intent.json");
            let chunk_name = name(i, "bin");
            let receipt_name = name(i, "receipt.json");
            if !names.remove(&intent_name) {
                break;
            }
            let intent: ChunkRecord = decode(&store.read(&intent_name, MAX_RECORD_BYTES as u64)?)?;
            ensure!(
                intent.schema == "oev2.nx-transfer-intent/1"
                    && intent.binding == binding
                    && intent.index == i,
                "transfer_store_intent_differs"
            );
            ensure!(
                names.remove(&chunk_name),
                "transfer_store_incomplete_write_held"
            );
            let data = store.read(&chunk_name, plan.budget.max_chunk_bytes)?;
            intent.payload.verify(&data)?;
            receiver.accept(&binding, i, &data)?;
            let mut wanted = intent;
            wanted.schema = "oev2.nx-transfer-receipt/1".into();
            if names.remove(&receipt_name) {
                let actual: ChunkRecord =
                    decode(&store.read(&receipt_name, MAX_RECORD_BYTES as u64)?)?;
                ensure!(actual == wanted, "transfer_store_receipt_differs");
            } else {
                ensure!(
                    explicit_commit,
                    "transfer_store_complete_chunk_requires_commit"
                );
                let raw = bytes(&wanted)?;
                store.create_sync(&receipt_name, &raw)?;
                ensure!(
                    store.read(&receipt_name, MAX_RECORD_BYTES as u64)? == raw,
                    "transfer_store_receipt_readback"
                );
            }
            next += 1;
        }
        ensure!(
            names.is_empty(),
            "transfer_store_unexpected_or_gapped_files"
        );
        store.verify()?;
        Ok(Self {
            store,
            plan,
            binding,
            receiver: Some(receiver),
            next,
            valid: true,
        })
    }
    fn verify_storage(&self, full: bool) -> Result<()> {
        if full {
            self.store.verify()
        } else {
            self.store.verify_scope()
        }
    }
    pub fn next_index(&self) -> usize {
        self.next
    }
    pub fn receive(&mut self, binding: &Digest, index: usize, data: &[u8]) -> Result<()> {
        ensure!(self.valid, "transfer_store_session_terminal");
        let result = self.receive_inner(binding, index, data, true);
        if result.is_err() {
            self.valid = false;
        }
        result
    }
    fn receive_inner(
        &mut self,
        binding: &Digest,
        index: usize,
        data: &[u8],
        full: bool,
    ) -> Result<()> {
        self.verify_storage(full)?;
        ensure!(
            index == self.next && binding == &self.binding,
            "transfer_store_binding_or_order"
        );
        self.receiver
            .as_mut()
            .context("transfer_store_receiver_missing")?
            .accept(binding, index, data)?;
        let intent = ChunkRecord {
            schema: "oev2.nx-transfer-intent/1".into(),
            binding: self.binding.clone(),
            index,
            payload: BlobIdentity::of(data),
        };
        self.store
            .create_sync(&name(index, "intent.json"), &bytes(&intent)?)?;
        self.store.create_sync(&name(index, "bin"), data)?;
        let readback = self
            .store
            .read(&name(index, "bin"), self.plan.budget.max_chunk_bytes)?;
        intent.payload.verify(&readback)?;
        let mut receipt = intent;
        receipt.schema = "oev2.nx-transfer-receipt/1".into();
        let raw = bytes(&receipt)?;
        self.store.create_sync(&name(index, "receipt.json"), &raw)?;
        ensure!(
            self.store
                .read(&name(index, "receipt.json"), MAX_RECORD_BYTES as u64)?
                == raw,
            "transfer_store_receipt_readback"
        );
        self.verify_storage(full)?;
        self.next += 1;
        Ok(())
    }
    /// One bounded frame carries consecutive plan spans. Preflight the entire
    /// range and length before any writes. A later write/hash failure may leave
    /// a verified prefix; inspect the same session instead of replaying blindly.
    pub fn receive_batch(
        &mut self,
        binding: &Digest,
        index: usize,
        count: usize,
        data: &[u8],
    ) -> Result<()> {
        ensure!(self.valid, "transfer_store_session_terminal");
        let result = self.receive_batch_inner(binding, index, count, data);
        if result.is_err() {
            self.valid = false;
        }
        result
    }
    fn receive_batch_inner(
        &mut self,
        binding: &Digest,
        index: usize,
        count: usize,
        data: &[u8],
    ) -> Result<()> {
        ensure!(
            binding == &self.binding && index == self.next,
            "transfer_store_binding_or_order"
        );
        ensure!(
            count > 0 && count <= 4096 && data.len() as u64 <= oe_nx_protocol::MAX_PAYLOAD_BYTES,
            "transfer_batch_bound"
        );
        let end = index.checked_add(count).context("transfer_batch_range")?;
        let spans = self
            .plan
            .spans
            .get(index..end)
            .context("transfer_batch_range")?;
        let lengths: Vec<usize> = spans
            .iter()
            .map(|s| usize::try_from(s.bytes))
            .collect::<std::result::Result<_, _>>()?;
        let total = lengths
            .iter()
            .try_fold(0usize, |sum, n| sum.checked_add(*n))
            .context("transfer_batch_length_overflow")?;
        ensure!(total == data.len(), "transfer_batch_length");
        self.store.verify()?;
        let mut offset = 0;
        for (part, length) in lengths.into_iter().enumerate() {
            self.receive_inner(binding, index + part, &data[offset..offset + length], false)?;
            offset += length;
        }
        self.store.verify()?;
        Ok(())
    }
    pub fn finish(mut self) -> Result<VerifiedCoverage> {
        ensure!(self.valid, "transfer_store_session_terminal");
        self.receiver
            .take()
            .context("transfer_store_receiver_missing")?
            .finish()?;
        // Fresh full reread detects changes to chunks acknowledged earlier.
        let mut reopened = Self::open_existing(self.store, self.plan)?;
        reopened
            .receiver
            .take()
            .context("transfer_store_receiver_missing")?
            .finish()
    }
}
