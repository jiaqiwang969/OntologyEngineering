//! Diagnostic reading of a failed/unknown task. Never grants sealed success.
use anyhow::{Result, ensure};
use oe_contracts::{NxMotionEvidenceBinding, NxMotionEvidenceBudget, NxMotionEvidenceEvent};
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader, Read};

pub(super) fn visit(
    budget: &NxMotionEvidenceBudget,
    binding: &NxMotionEvidenceBinding,
    names: &[String],
    mut source: impl FnMut(&str, &mut dyn FnMut(&mut dyn Read) -> Result<()>) -> Result<()>,
    visitor: &mut dyn FnMut(&NxMotionEvidenceEvent) -> Result<()>,
) -> Result<bool> {
    super::super::validate_stream_inputs(budget, binding)?;
    ensure!(
        names.len() as u64 <= budget.max_segments,
        "motion2_prefix_segment_bound"
    );
    let (mut count, mut total) = (0u64, 0u64);
    let mut previous = "0".repeat(64);
    let mut sealed = false;
    let mut truncated = false;
    for (index, name) in names.iter().enumerate() {
        ensure!(
            *name == super::super::segment_name(index as u64),
            "motion2_prefix_segment_inventory"
        );
        let (mut calls, mut complete, mut bytes) = (0usize, false, 0u64);
        source(name, &mut |reader| {
            calls += 1;
            ensure!(calls == 1, "motion2_prefix_repeated_reader");
            let mut reader = BufReader::new(reader);
            loop {
                let mut line = Vec::new();
                let size = Read::take(&mut reader, budget.max_event_bytes + 1)
                    .read_until(b'\n', &mut line)?;
                if size == 0 {
                    break;
                }
                let size = size as u64;
                ensure!(
                    !sealed
                        && size <= budget.max_event_bytes
                        && size <= budget.segment_bytes.saturating_sub(bytes)
                        && size <= budget.max_bytes.saturating_sub(total)
                        && count < budget.max_events,
                    "motion2_prefix_byte_or_event_bound"
                );
                bytes += size;
                total += size;
                if line.last() != Some(&b'\n') {
                    ensure!(
                        index + 1 == names.len(),
                        "motion2_partial_line_not_final_segment"
                    );
                    truncated = true;
                    break;
                }
                let event: NxMotionEvidenceEvent =
                    serde_json::from_value(oe_nx_protocol::json::strict_json(&line)?)?;
                ensure!(
                    event.schema == "oev2.nx-motion-evidence-event/1"
                        && event.sequence == count + 1
                        && event.previous_sha256 == previous,
                    "motion2_prefix_event_chain"
                );
                oe_nx_protocol::checked_token(&event.phase)?;
                if count == 0 {
                    ensure!(
                        event.phase == "stream_open"
                            && event.observation == serde_json::to_value(binding)?,
                        "motion2_prefix_open_binding"
                    );
                } else {
                    ensure!(event.phase != "stream_open", "motion2_prefix_second_open");
                    if event.phase == "stream_seal" {
                        ensure!(
                            event.observation == serde_json::json!({"committed_before_seal":count}),
                            "motion2_prefix_seal"
                        );
                        sealed = true;
                    }
                }
                visitor(&event)?;
                previous = format!("{:x}", Sha256::digest(&line));
                count += 1;
            }
            complete = true;
            Ok(())
        })?;
        ensure!(calls == 1 && complete, "motion2_prefix_source_incomplete");
        // The source adapter still must verify each handle/file identity after
        // EOF, including an incomplete last line. It is not a writer summary.
    }
    Ok(truncated)
}
