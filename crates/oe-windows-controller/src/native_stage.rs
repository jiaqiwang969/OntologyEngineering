//! Actual full-payload review and atomic pre-dispatch v3 stage commit.
//! Recovery is explicit; verified temporary originals are retained by rename.
use crate::{
    adapter, transfer_reconstruct::Target, transfer_reconstruct_windows::PinnedTarget, windows::*,
};
use anyhow::{Context, Result, ensure};
use oe_nx_protocol::{
    self as p,
    published_payload::{PreparedPayload, PublishedPayloadPlan},
};
use std::{collections::BTreeSet, ffi::OsStr};
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Commit,
    Inspect,
    Recover,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Checkpoint {
    ReceiptSynced,
    PendingSynced,
    TemporarySynced,
    ReplacedAndReadback,
    HistoryRenamed,
}
fn stem<'a>(name: &'a str, prefix: &str, suffix: &str) -> Option<&'a str> {
    let s = name.strip_prefix(prefix)?.strip_suffix(suffix)?;
    let p = s.split('-').collect::<Vec<_>>();
    (p.len() == 2
        && p.iter().all(|n| {
            !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) && n.parse::<u64>().is_ok()
        }))
    .then_some(s)
}
struct Controls {
    receipt: bool,
    state: bool,
    pending: bool,
    temporary: Vec<String>,
    entries: Vec<p::FileEntry>,
    pins: Vec<PinnedFile>,
}
fn controls(
    dir: &PinnedDirectory<'_>,
    core: &[p::FileEntry],
    receipt: &[u8],
    state: &[u8],
) -> Result<Controls> {
    let top: BTreeSet<_> = core
        .iter()
        .map(|f| f.path.as_str().split('/').next().unwrap().to_owned())
        .collect();
    let mut out = Controls {
        receipt: false,
        state: false,
        pending: false,
        temporary: vec![],
        entries: vec![],
        pins: vec![],
    };
    for entry in std::fs::read_dir(dir.path())? {
        let name = entry?
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("stage_nonunicode_member"))?;
        if top.contains(&name) {
            continue;
        }
        ensure!(out.entries.len() < 64, "stage_control_count_bound");
        let expected = if name == "stage-receipt.json" {
            out.receipt = true;
            receipt
        } else if name == STATE_NAME {
            out.state = true;
            state
        } else if name == PENDING_NAME {
            out.pending = true;
            state
        } else if stem(&name, ".nx-state-", ".tmp").is_some() {
            out.temporary.push(name.clone());
            state
        } else if stem(&name, "stage-recovery-", ".json").is_some() {
            state
        } else {
            anyhow::bail!("stage_unknown_control_member")
        };
        let mut pin = dir
            .pin_file(OsStr::new(&name), p::MAX_RECORD_BYTES as u64)?
            .context("stage_record_disappeared")?;
        ensure!(
            pin.read_all(p::MAX_RECORD_BYTES as u64)? == expected,
            "stage_control_bytes_differ"
        );
        pin.verify()?;
        out.entries.push(p::FileEntry::from_bytes(&name, expected)?);
        out.pins.push(pin);
    }
    ensure!(
        (!out.state && !out.pending && out.temporary.is_empty()) || out.receipt,
        "stage_state_without_receipt"
    );
    dir.verify()?;
    Ok(out)
}
pub fn stage(
    dir: &PinnedDirectory<'_>,
    lock: &WriterLock<'_>,
    job: &p::JobManifest,
    plan: &PublishedPayloadPlan,
    prepared: &PreparedPayload,
    op: Operation,
) -> Result<p::StateRecord> {
    stage_with_checkpoint(dir, lock, job, plan, prepared, op, |_| Ok(()))
}
pub fn stage_with_checkpoint<F: FnMut(Checkpoint) -> std::io::Result<()>>(
    dir: &PinnedDirectory<'_>,
    lock: &WriterLock<'_>,
    job: &p::JobManifest,
    plan: &PublishedPayloadPlan,
    prepared: &PreparedPayload,
    op: Operation,
    mut checkpoint: F,
) -> Result<p::StateRecord> {
    prepared.validate(job, plan)?;
    ensure!(
        job.root == adapter::root_identity(lock.root())
            && dir.path().file_name() == Some(OsStr::new(job.job_id.as_str())),
        "stage_actual_job_root_differs"
    );
    let mut tree = PinnedTarget::new(dir, lock)?;
    ensure!(
        tree.identity()? == prepared.target,
        "stage_actual_directory_differs"
    );
    let mut core = job.payload_files();
    core.push(p::FileEntry::from_bytes(
        "job.json",
        &serde_json::to_vec(job)?,
    )?);
    core.push(p::FileEntry::from_bytes(
        "source-plan.json",
        &serde_json::to_vec(plan)?,
    )?);
    for file in &core {
        ensure!(
            tree.readback(file)? == file.identity,
            "stage_payload_changed"
        );
    }
    let mut params = adapter::pin_relative_file(dir, &job.params.path, p::MAX_RECORD_BYTES as u64)?;
    job.validate_parameters(&params.read_all(p::MAX_RECORD_BYTES as u64)?)?;
    let mut wanted = p::StateRecord::prepared(job)?;
    wanted.mark_staged_published(job, plan, prepared)?;
    let state_bytes = serde_json::to_vec(&wanted)?;
    let receipt_bytes = serde_json::to_vec(prepared)?;
    let observed = controls(dir, &core, &receipt_bytes, &state_bytes)?;
    tree.verify_with_control(&core, &observed.entries)?;
    let committed =
        observed.receipt && observed.state && !observed.pending && observed.temporary.is_empty();
    if !committed {
        ensure!(op != Operation::Inspect, "stage_not_committed");
        ensure!(
            op == Operation::Recover
                || (!observed.receipt
                    && !observed.state
                    && !observed.pending
                    && observed.temporary.is_empty()),
            "stage_requires_explicit_recovery"
        );
        let had_receipt = observed.receipt;
        let had_state = observed.state;
        let pending = observed.pending;
        let temporary = observed.temporary.clone();
        drop(observed);
        if !had_receipt {
            dir.create_file_new(
                OsStr::new("stage-receipt.json"),
                &receipt_bytes,
                p::MAX_RECORD_BYTES as u64,
            )?;
            let mut pin = dir
                .pin_file(OsStr::new("stage-receipt.json"), p::MAX_RECORD_BYTES as u64)?
                .context("stage_receipt_missing")?;
            ensure!(
                pin.read_all(p::MAX_RECORD_BYTES as u64)? == receipt_bytes,
                "stage_receipt_readback_differs"
            );
            pin.verify()?;
            checkpoint(Checkpoint::ReceiptSynced)?;
        }
        for name in temporary {
            let suffix =
                stem(&name, ".nx-state-", ".tmp").context("stage_temporary_name_changed")?;
            let archive = format!("stage-recovery-{suffix}.json");
            rename_record_new(
                dir,
                lock,
                OsStr::new(&name),
                OsStr::new(&archive),
                &state_bytes,
            )?;
            checkpoint(Checkpoint::HistoryRenamed)?;
        }
        if pending {
            recover_state_new(dir, lock, &state_bytes)?;
        } else if !had_state {
            write_state_with_checkpoint(dir, lock, None, &state_bytes, |point| {
                checkpoint(match point {
                    WriteCheckpoint::PendingSynced => Checkpoint::PendingSynced,
                    WriteCheckpoint::TemporarySynced => Checkpoint::TemporarySynced,
                    WriteCheckpoint::ReplacedAndReadback => Checkpoint::ReplacedAndReadback,
                })
            })?;
        }
    }
    let final_records = controls(dir, &core, &receipt_bytes, &state_bytes)?;
    ensure!(
        final_records.receipt
            && final_records.state
            && !final_records.pending
            && final_records.temporary.is_empty(),
        "stage_commit_incomplete"
    );
    tree.verify_with_control(&core, &final_records.entries)?;
    for pin in final_records.pins {
        pin.verify()?;
    }
    params.verify()?;
    lock.verify()?;
    wanted.validate_published_stage(job, plan, prepared, &tree.identity()?)?;
    Ok(wanted)
}
