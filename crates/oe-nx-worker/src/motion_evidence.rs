//! Bounded segmented persistence. A sealed stream is not a completed CAD task.
use anyhow::{Result, ensure};
use oe_contracts::{
    NxMotionEvidenceBinding, NxMotionEvidenceBudget, NxMotionEvidenceEvent,
    NxMotionEvidenceSegment, NxMotionEvidenceSummary,
};
use sha2::{Digest, Sha256};
use std::io::{self, Write};
pub trait DurableWrite: Write {
    fn synchronize(&mut self) -> io::Result<()>;
}
impl DurableWrite for std::fs::File {
    fn synchronize(&mut self) -> io::Result<()> {
        self.sync_all()
    }
}
pub fn segment_name(index: u64) -> String {
    format!("motion-events-{index:06}.jsonl")
}
struct Limited {
    data: Vec<u8>,
    limit: usize,
}
impl Write for Limited {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        if b.len() > self.limit.saturating_sub(self.data.len()) {
            return Err(io::Error::other("motion_event_bytes_exceeded"));
        }
        self.data.extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
struct Active<W> {
    writer: W,
    index: u64,
    first: u64,
    bytes: u64,
    hash: Sha256,
}
pub struct EvidenceWriter<W: DurableWrite, F: FnMut(u64) -> Result<W>> {
    factory: F,
    budget: NxMotionEvidenceBudget,
    active: Option<Active<W>>,
    segments: Vec<NxMotionEvidenceSegment>,
    events: u64,
    bytes: u64,
    hash: Sha256,
    previous: String,
    poisoned: bool,
    sealed: bool,
}
impl<W: DurableWrite, F: FnMut(u64) -> Result<W>> EvidenceWriter<W, F> {
    pub fn new(
        factory: F,
        budget: NxMotionEvidenceBudget,
        binding: NxMotionEvidenceBinding,
    ) -> Result<Self> {
        validate_stream_inputs(&budget, &binding)?;
        let mut s = Self {
            factory,
            budget,
            active: None,
            segments: vec![],
            events: 0,
            bytes: 0,
            hash: Sha256::new(),
            previous: "0".repeat(64),
            poisoned: false,
            sealed: false,
        };
        s.push("stream_open", serde_json::to_value(binding)?)?;
        Ok(s)
    }
    /// Exact read-only append admission. Never writes, advances counters or poisons
    /// the writer. A true answer reserves no capacity; append remains authoritative.
    pub fn can_append(&self, phase: &str, observation: &serde_json::Value) -> Result<bool> {
        ensure!(
            !self.poisoned && !self.sealed,
            "motion_evidence_writer_not_open"
        );
        ensure!(
            phase != "stream_open" && phase != "stream_seal",
            "motion_reserved_stream_phase"
        );
        oe_nx_protocol::checked_token(phase)?;
        if self.events >= self.budget.max_events {
            return Ok(false);
        }
        let event = NxMotionEvidenceEvent {
            schema: "oev2.nx-motion-evidence-event/1".into(),
            sequence: self.events + 1,
            previous_sha256: self.previous.clone(),
            phase: phase.into(),
            observation: observation.clone(),
        };
        let mut raw = Limited {
            data: Vec::new(),
            limit: self.budget.max_event_bytes as usize,
        };
        if serde_json::to_writer(&mut raw, &event).is_err() || raw.write_all(b"\n").is_err() {
            return Ok(false);
        }
        let size = raw.data.len() as u64;
        if size > self.budget.max_bytes.saturating_sub(self.bytes)
            || size > self.budget.segment_bytes
        {
            return Ok(false);
        }
        let needs_next = self
            .active
            .as_ref()
            .is_some_and(|a| size > self.budget.segment_bytes.saturating_sub(a.bytes));
        let index = self.segments.len() as u64 + u64::from(needs_next);
        Ok(index < self.budget.max_segments)
    }
    pub fn committed_events(&self) -> u64 {
        self.events
    }
    pub fn poisoned(&self) -> bool {
        self.poisoned
    }
    pub fn append(&mut self, phase: &str, observation: serde_json::Value) -> Result<()> {
        if phase == "stream_open" || phase == "stream_seal" {
            self.poisoned = true;
            anyhow::bail!("motion_reserved_stream_phase");
        }
        self.push(phase, observation)
    }
    fn close_segment(&mut self) {
        if let Some(a) = self.active.take() {
            self.segments.push(NxMotionEvidenceSegment {
                index: a.index,
                path: segment_name(a.index),
                first_sequence: a.first,
                last_sequence: self.events,
                bytes: a.bytes,
                sha256: format!("{:x}", a.hash.finalize()),
            });
        }
    }
    fn push(&mut self, phase: &str, observation: serde_json::Value) -> Result<()> {
        ensure!(
            !self.poisoned && !self.sealed,
            "motion_evidence_writer_not_open"
        );
        self.poisoned = true;
        let result = self.write_event(phase, observation);
        if result.is_ok() {
            self.poisoned = false;
        }
        result
    }
    fn write_event(&mut self, phase: &str, observation: serde_json::Value) -> Result<()> {
        oe_nx_protocol::checked_token(phase)?;
        ensure!(
            self.events < self.budget.max_events,
            "motion_event_count_exceeded"
        );
        let event = NxMotionEvidenceEvent {
            schema: "oev2.nx-motion-evidence-event/1".into(),
            sequence: self.events + 1,
            previous_sha256: self.previous.clone(),
            phase: phase.into(),
            observation,
        };
        let mut raw = Limited {
            data: Vec::new(),
            limit: self.budget.max_event_bytes as usize,
        };
        serde_json::to_writer(&mut raw, &event)?;
        raw.write_all(b"\n")?;
        let size = raw.data.len() as u64;
        ensure!(
            size <= self.budget.max_bytes - self.bytes,
            "motion_total_bytes_exceeded"
        );
        if self
            .active
            .as_ref()
            .is_some_and(|a| size > self.budget.segment_bytes - a.bytes)
        {
            self.close_segment();
        }
        if self.active.is_none() {
            let index = self.segments.len() as u64;
            ensure!(
                index < self.budget.max_segments,
                "motion_segment_count_exceeded"
            );
            self.active = Some(Active {
                writer: (self.factory)(index)?,
                index,
                first: self.events + 1,
                bytes: 0,
                hash: Sha256::new(),
            });
        }
        let active = self.active.as_mut().unwrap();
        active.writer.write_all(&raw.data)?;
        active.writer.synchronize()?;
        // No committed counter or digest changes until durable write succeeds.
        active.bytes += size;
        active.hash.update(&raw.data);
        self.hash.update(&raw.data);
        self.previous = format!("{:x}", Sha256::digest(&raw.data));
        self.bytes += size;
        self.events += 1;
        Ok(())
    }
    pub fn seal(&mut self) -> Result<NxMotionEvidenceSummary> {
        self.push(
            "stream_seal",
            serde_json::json!({"committed_before_seal":self.events}),
        )?;
        self.close_segment();
        self.sealed = true;
        Ok(NxMotionEvidenceSummary {
            schema: "oev2.nx-motion-evidence-summary/1".into(),
            events: self.events,
            bytes: self.bytes,
            sha256: format!("{:x}", self.hash.clone().finalize()),
            segments: self.segments.clone(),
            sealed: true,
        })
    }
}

pub fn validate_stream_inputs(
    budget: &NxMotionEvidenceBudget,
    binding: &NxMotionEvidenceBinding,
) -> Result<()> {
    ensure!(
        (2..=1_000_000_000).contains(&budget.max_events)
            && (1..=100 * 1024 * 1024).contains(&budget.segment_bytes)
            && (1..=4 * 1024 * 1024).contains(&budget.max_event_bytes)
            && budget.max_event_bytes <= budget.segment_bytes
            && budget.max_bytes >= budget.max_event_bytes
            && (1..=4096).contains(&budget.max_segments)
            && budget.max_bytes
                <= budget
                    .segment_bytes
                    .checked_mul(budget.max_segments)
                    .unwrap(),
        "motion_evidence_budget_invalid"
    );
    binding
        .validate()
        .map_err(|_| anyhow::anyhow!("motion_evidence_binding_invalid"))?;
    oe_nx_protocol::checked_token(&binding.job_id)?;
    oe_nx_protocol::RelativePath::new(binding.intent.path.clone())?;
    oe_nx_protocol::Digest::new(binding.intent.sha256.clone())?;
    ensure!(
        (1..=4096).contains(&binding.samples),
        "motion_evidence_sample_count_invalid"
    );
    Ok(())
}

pub mod receiving;
pub mod receiving2;

#[cfg(windows)]
pub mod windows;

pub mod sink;
