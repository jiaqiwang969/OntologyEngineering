//! Read originals, not writer assertions. Successful stream verification never
//! implies complete motion coverage, saved CAD persistence or engineering use.
use super::{segment_name, validate_stream_inputs};
use anyhow::{Result, ensure};
use oe_contracts::{
    NxMotionEvidenceBinding, NxMotionEvidenceBudget, NxMotionEvidenceEvent, NxMotionEvidenceSummary,
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    io::{BufRead, BufReader, Read},
};
/// Adapter must enumerate an owned namespace and call the visitor exactly once
/// per requested, pinned regular file. It must verify file identity after EOF
/// before returning success. This supports existing PinnedFile::reader/verify
/// without loading a complete segment into memory or weakening its handle pins.
pub fn verify_evidence(
    summary: &NxMotionEvidenceSummary,
    budget: &NxMotionEvidenceBudget,
    binding: &NxMotionEvidenceBinding,
    observed_names: &[String],
    source: impl FnMut(&str, &mut dyn FnMut(&mut dyn Read) -> Result<()>) -> Result<()>,
) -> Result<()> {
    verify_evidence_with(
        summary,
        budget,
        binding,
        observed_names,
        source,
        &mut |_| Ok(()),
    )
}

/// Visitor effects must remain provisional until this function returns success:
/// segment/file and whole-stream identities are checked after the observations.
/// The original verify_evidence is exactly this path with a no-op visitor.
pub fn verify_evidence_with(
    summary: &NxMotionEvidenceSummary,
    budget: &NxMotionEvidenceBudget,
    binding: &NxMotionEvidenceBinding,
    observed_names: &[String],
    mut source: impl FnMut(&str, &mut dyn FnMut(&mut dyn Read) -> Result<()>) -> Result<()>,
    visitor: &mut dyn FnMut(&NxMotionEvidenceEvent) -> Result<()>,
) -> Result<()> {
    validate_stream_inputs(budget, binding)?;
    ensure!(
        summary.schema == "oev2.nx-motion-evidence-summary/1"
            && summary.sealed
            && !summary.segments.is_empty()
            && summary.segments.len() as u64 <= budget.max_segments
            && (2..=budget.max_events).contains(&summary.events)
            && summary.bytes <= budget.max_bytes,
        "motion_received_summary_invalid"
    );
    ensure!(
        observed_names.len() == summary.segments.len(),
        "motion_received_segment_inventory_differs"
    );
    let declared = summary
        .segments
        .iter()
        .try_fold(0u64, |sum, s| sum.checked_add(s.bytes))
        .ok_or_else(|| anyhow::anyhow!("motion_received_size_overflow"))?;
    ensure!(
        declared == summary.bytes && declared > 0,
        "motion_received_declared_total_differs"
    );
    oe_nx_protocol::Digest::new(summary.sha256.clone())?;
    let names = observed_names.iter().cloned().collect::<BTreeSet<_>>();
    ensure!(
        names.len() == observed_names.len() && names.len() == summary.segments.len(),
        "motion_received_segment_inventory_differs"
    );
    let expected_binding = serde_json::to_value(binding)?;
    let (mut count, mut total) = (0u64, 0u64);
    let mut previous = "0".repeat(64);
    let mut all_hash = Sha256::new();
    let mut sealed = false;
    for (index, segment) in summary.segments.iter().enumerate() {
        ensure!(
            segment.index == index as u64
                && segment.path == segment_name(index as u64)
                && names.contains(&segment.path)
                && segment.first_sequence == count + 1
                && (1..=budget.segment_bytes).contains(&segment.bytes),
            "motion_received_segment_metadata_invalid"
        );
        oe_nx_protocol::Digest::new(segment.sha256.clone())?;
        let mut calls = 0u64;
        let mut visit_completed = false;
        let mut bytes = 0u64;
        let mut hash = Sha256::new();
        source(&segment.path, &mut |reader| {
            calls = calls.saturating_add(1);
            ensure!(calls == 1, "motion_received_source_called_twice");
            let mut reader = BufReader::new(reader);
            loop {
                let mut line = Vec::new();
                let size = Read::take(&mut reader, budget.max_event_bytes + 1)
                    .read_until(b'\n', &mut line)?;
                if size == 0 {
                    break;
                }
                ensure!(
                    !sealed && size as u64 <= budget.max_event_bytes && line.last() == Some(&b'\n'),
                    "motion_received_unbounded_partial_or_post_seal_event"
                );
                ensure!(
                    size as u64 <= segment.bytes - bytes
                        && size as u64 <= budget.max_bytes - total
                        && count < summary.events
                        && count < budget.max_events,
                    "motion_received_resource_bound"
                );
                let event: NxMotionEvidenceEvent =
                    serde_json::from_value(oe_nx_protocol::json::strict_json(&line)?)?;
                ensure!(
                    event.schema == "oev2.nx-motion-evidence-event/1"
                        && event.sequence == count + 1
                        && event.previous_sha256 == previous,
                    "motion_received_event_chain_differs"
                );
                oe_nx_protocol::checked_token(&event.phase)?;
                if count == 0 {
                    ensure!(
                        event.phase == "stream_open" && event.observation == expected_binding,
                        "motion_received_binding_differs"
                    );
                } else {
                    ensure!(event.phase != "stream_open", "motion_received_second_open");
                    if event.phase == "stream_seal" {
                        ensure!(
                            event.observation == serde_json::json!({"committed_before_seal":count}),
                            "motion_received_seal_differs"
                        );
                        sealed = true;
                    }
                }
                visitor(&event)?;
                previous = format!("{:x}", Sha256::digest(&line));
                hash.update(&line);
                all_hash.update(&line);
                bytes += size as u64;
                total += size as u64;
                count += 1;
            }
            visit_completed = true;
            Ok(())
        })?;
        ensure!(
            calls == 1
                && visit_completed
                && bytes == segment.bytes
                && segment.last_sequence == count
                && format!("{:x}", hash.finalize()) == segment.sha256,
            "motion_received_segment_content_differs"
        );
    }
    ensure!(
        sealed
            && count == summary.events
            && total == summary.bytes
            && format!("{:x}", all_hash.finalize()) == summary.sha256,
        "motion_received_whole_stream_differs"
    );
    Ok(())
}
