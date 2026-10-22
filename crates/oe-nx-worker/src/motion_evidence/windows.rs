//! Task-owned Windows directory adapter. Pins survive all segment reads.
use super::{EvidenceWriter, receiving::verify_evidence, segment_name};
use anyhow::{Context, Result, ensure};
use oe_contracts::{NxMotionEvidenceBinding, NxMotionEvidenceBudget, NxMotionEvidenceSummary};
use oe_windows_controller::windows::PinnedDirectory;
use std::{ffi::OsStr, fs::File};
fn names(directory: &PinnedDirectory<'_>, limit: u64) -> Result<Vec<String>> {
    directory.verify()?;
    let mut out = Vec::new();
    for entry in std::fs::read_dir(directory.path())? {
        ensure!(
            (out.len() as u64) < limit,
            "motion_owned_stream_entry_budget"
        );
        out.push(
            entry?
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("motion_stream_name_not_unicode"))?,
        );
    }
    out.sort();
    directory.verify()?;
    Ok(out)
}
pub fn create_owned_writer<'a, 'root>(
    directory: &'a PinnedDirectory<'root>,
    budget: NxMotionEvidenceBudget,
    binding: NxMotionEvidenceBinding,
) -> Result<EvidenceWriter<File, impl FnMut(u64) -> Result<File> + 'a>> {
    super::validate_stream_inputs(&budget, &binding)?;
    ensure!(
        names(directory, 1)?.is_empty(),
        "motion_stream_directory_not_empty"
    );
    EvidenceWriter::new(
        move |index| Ok(directory.create_log_new(OsStr::new(&segment_name(index)))?),
        budget,
        binding,
    )
}
pub fn verify_owned_stream(
    directory: &PinnedDirectory<'_>,
    summary: &NxMotionEvidenceSummary,
    budget: &NxMotionEvidenceBudget,
    binding: &NxMotionEvidenceBinding,
) -> Result<()> {
    super::validate_stream_inputs(budget, binding)?;
    let before = names(directory, budget.max_segments)?;
    let mut pins = Vec::new();
    verify_evidence(summary, budget, binding, &before, |name, visit| {
        let mut pin = directory
            .pin_file(OsStr::new(name), budget.segment_bytes)?
            .context("motion_stream_segment_missing")?;
        visit(pin.reader()?)?;
        pin.verify()?;
        pins.push(pin);
        Ok(())
    })?;
    ensure!(
        before == names(directory, budget.max_segments)?,
        "motion_stream_directory_changed"
    );
    for pin in &pins {
        pin.verify()?;
    }
    directory.verify()?;
    Ok(())
}

/// The task retains the directory; this borrowed sink owns only the active file.
pub fn create_owned_sink<'a, 'root>(
    directory: &'a PinnedDirectory<'root>,
    budget: NxMotionEvidenceBudget,
    binding: NxMotionEvidenceBinding,
) -> Result<Box<dyn super::sink::EvidenceSink + 'a>> {
    let writer = create_owned_writer(directory, budget.clone(), binding.clone())?;
    Ok(Box::new(super::sink::VerifiedSink::new(
        writer,
        move |summary: &NxMotionEvidenceSummary| {
            verify_owned_stream(directory, summary, &budget, &binding)
        },
    )))
}
