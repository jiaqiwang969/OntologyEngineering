//! Publication is a committed metadata pointer over freshly verified payloads.
//! Every use rechecks payloads; a published source is not an NX execution grant.
use crate::{
    adapter,
    transfer_reconstruct::{Target, validate_plan},
    transfer_reconstruct_windows::PinnedTarget,
    windows::*,
};
use anyhow::{Context, Result, ensure};
use oe_nx_protocol::{self as p, segmented::TransferPlan, transfer::SourcePublication};
use std::ffi::OsStr;
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Publish,
    Inspect,
    Recover,
}
fn temporary(name: &str) -> bool {
    let Some(s) = name
        .strip_prefix(".nx-state-")
        .and_then(|s| s.strip_suffix(".tmp"))
    else {
        return false;
    };
    let parts: Vec<_> = s.split('-').collect();
    parts.len() == 2
        && parts
            .iter()
            .all(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
}
pub(crate) fn records(
    dir: &PinnedDirectory<'_>,
    wanted: &[u8],
) -> Result<(bool, bool, Vec<PinnedFile>)> {
    let mut state = false;
    let mut pending = false;
    let mut pins = Vec::new();
    for e in std::fs::read_dir(dir.path())? {
        ensure!(pins.len() < 64, "publication_record_namespace_bound");
        let e = e?;
        let name = e
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("publication_nonunicode_name"))?;
        ensure!(
            name == STATE_NAME || name == PENDING_NAME || temporary(&name),
            "publication_unexpected_entry"
        );
        let mut pin = dir
            .pin_file(OsStr::new(&name), 64 * 1024)?
            .context("publication_record_disappeared")?;
        ensure!(
            pin.read_all(64 * 1024)? == wanted,
            "publication_record_bytes_differ"
        );
        pin.verify()?;
        state |= name == STATE_NAME;
        pending |= name == PENDING_NAME;
        pins.push(pin);
    }
    dir.verify()?;
    Ok((state, pending, pins))
}
pub fn publish(
    root: &StableRoot,
    lock: &WriterLock<'_>,
    plan: &TransferPlan,
    output_id: &p::JobId,
    publication_id: &p::JobId,
    expected_target: &p::RootIdentity,
    operation: Operation,
) -> Result<SourcePublication> {
    publish_with_checkpoint(
        root,
        lock,
        plan,
        output_id,
        publication_id,
        expected_target,
        operation,
        |_| Ok(()),
    )
}
pub fn publish_with_checkpoint<F: FnMut(WriteCheckpoint) -> std::io::Result<()>>(
    root: &StableRoot,
    lock: &WriterLock<'_>,
    plan: &TransferPlan,
    output_id: &p::JobId,
    publication_id: &p::JobId,
    expected_target: &p::RootIdentity,
    operation: Operation,
    mut checkpoint: F,
) -> Result<SourcePublication> {
    validate_plan(plan)?;
    ensure!(
        plan.root == adapter::root_identity(root),
        "publication_source_root"
    );
    ensure!(
        output_id != &plan.job_id && publication_id != output_id && publication_id != &plan.job_id,
        "publication_namespace_alias"
    );
    let output = root.directory(OsStr::new(output_id.as_str()))?;
    let mut target = PinnedTarget::new(&output, lock)?;
    ensure!(
        target.identity()? == *expected_target,
        "publication_target_identity_differs"
    );
    for file in &plan.files {
        ensure!(
            target.readback(file)? == file.identity,
            "publication_payload_changed"
        );
    }
    target.verify_complete(&plan.files)?;
    let (directory, created) = match root.directory(OsStr::new(publication_id.as_str())) {
        Ok(d) => (d, false),
        Err(e) if operation == Operation::Publish && matches!(e.raw_os_error(), Some(2 | 3)) => (
            root.create_directory_new(OsStr::new(publication_id.as_str()))?,
            true,
        ),
        Err(e) => return Err(e.into()),
    };
    let mut publication_root = adapter::root_identity(root);
    publication_root.volume_id = directory.identity().volume_id.clone();
    publication_root.directory_id = directory.identity().file_id.clone();
    let result = SourcePublication {
        schema: "oev2.nx-source-publication/1".into(),
        source_root: plan.root.clone(),
        job_id: plan.job_id.clone(),
        plan_digest: plan.binding()?,
        output_id: output_id.clone(),
        publication_id: publication_id.clone(),
        publication_root,
        target: expected_target.clone(),
        inventory_digest: p::record_digest(&plan.files)?,
        files: plan.files.len(),
        bytes: plan.files.iter().map(|f| f.identity.bytes).sum(),
        state: "published".into(),
        native_execution_started: false,
    };
    let bytes = serde_json::to_vec(&result)?;
    if created {
        ensure!(
            std::fs::read_dir(directory.path())?.next().is_none(),
            "publication_new_directory_not_empty"
        );
        write_state_with_checkpoint(&directory, lock, None, &bytes, &mut checkpoint)?;
    } else {
        let (state, pending, pins) = records(&directory, &bytes)?;
        drop(pins);
        if pending {
            ensure!(
                operation == Operation::Recover,
                "publication_pending_requires_explicit_recovery"
            );
            recover_state_new(&directory, lock, &bytes)?;
        } else {
            ensure!(state, "publication_without_intent_or_state_held");
        }
    }
    let (state, pending, pins) = records(&directory, &bytes)?;
    ensure!(state && !pending, "publication_not_committed");
    target.verify_complete(&plan.files)?;
    ensure!(
        target.identity()? == *expected_target,
        "publication_target_changed"
    );
    for pin in pins {
        pin.verify()?;
    }
    lock.verify()?;
    Ok(result)
}
