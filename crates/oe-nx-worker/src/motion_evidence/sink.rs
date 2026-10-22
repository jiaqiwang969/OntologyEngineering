//! Object-safe task port. Sealing only succeeds after original-file verification.
use super::{DurableWrite, EvidenceWriter};
use anyhow::{Result, ensure};
use oe_contracts::NxMotionEvidenceSummary;
use serde_json::Value;

pub trait EvidenceSink {
    fn can_append(&self, phase: &str, observation: &Value) -> Result<bool>;
    fn append(&mut self, phase: &str, observation: Value) -> Result<()>;
    fn seal_and_verify(&mut self) -> Result<NxMotionEvidenceSummary>;
}

pub struct VerifiedSink<W: DurableWrite, F: FnMut(u64) -> Result<W>, V> {
    writer: EvidenceWriter<W, F>,
    verify: V,
    terminal: bool,
}
impl<W: DurableWrite, F: FnMut(u64) -> Result<W>, V: FnMut(&NxMotionEvidenceSummary) -> Result<()>>
    VerifiedSink<W, F, V>
{
    pub fn new(writer: EvidenceWriter<W, F>, verify: V) -> Self {
        Self {
            writer,
            verify,
            terminal: false,
        }
    }
}
impl<W: DurableWrite, F: FnMut(u64) -> Result<W>, V: FnMut(&NxMotionEvidenceSummary) -> Result<()>>
    EvidenceSink for VerifiedSink<W, F, V>
{
    fn can_append(&self, phase: &str, observation: &Value) -> Result<bool> {
        ensure!(!self.terminal, "motion_sink_terminal");
        self.writer.can_append(phase, observation)
    }
    fn append(&mut self, phase: &str, observation: Value) -> Result<()> {
        ensure!(!self.terminal, "motion_sink_terminal");
        if let Err(error) = self.writer.append(phase, observation) {
            self.terminal = true;
            return Err(error);
        }
        Ok(())
    }
    fn seal_and_verify(&mut self) -> Result<NxMotionEvidenceSummary> {
        ensure!(!self.terminal, "motion_sink_terminal");
        // Failed verification cannot be retried against changed originals.
        self.terminal = true;
        let summary = self.writer.seal()?;
        (self.verify)(&summary)?;
        Ok(summary)
    }
}

/// Native callbacks carry their own phase; reject malformed events before I/O.
pub fn record_native(sink: &mut dyn EvidenceSink, event: Value) -> Result<()> {
    let phase = event
        .get("phase")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("native_motion_event_phase_missing"))?
        .to_owned();
    sink.append(&phase, event)
}
