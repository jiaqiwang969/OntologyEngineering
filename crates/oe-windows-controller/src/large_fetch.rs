//! Read-only snapshots and pinned byte ranges. Whole-file delivery is checked by
//! the receiver; a range reply does not certify a complete file or native result.
use crate::{
    adapter,
    windows::{PinnedDirectory, PinnedFile, StableRoot},
};
use anyhow::{Context, Result, ensure};
use oe_nx_protocol::{self as p, large_fetch::*};
use std::{
    ffi::OsStr,
    io::{Read, Seek, SeekFrom},
};
fn target(root: &StableRoot, dir: &PinnedDirectory<'_>) -> p::RootIdentity {
    let mut id = adapter::root_identity(root);
    id.volume_id = dir.identity().volume_id.clone();
    id.directory_id = dir.identity().file_id.clone();
    id
}
fn job(
    root: &StableRoot,
    dir: &PinnedDirectory<'_>,
    id: &p::JobId,
    digest: &p::Digest,
) -> Result<(p::JobManifest, p::BlobIdentity, PinnedFile)> {
    let mut pin = dir
        .pin_file(OsStr::new("job.json"), p::MAX_RECORD_BYTES as u64)?
        .context("fetch_job_missing")?;
    let raw = pin.read_all(p::MAX_RECORD_BYTES as u64)?;
    let job: p::JobManifest = p::decode(&raw)?;
    job.validate()?;
    ensure!(
        job.root == adapter::root_identity(root) && job.job_id == *id && job.digest()? == *digest,
        "fetch_actual_job_differs"
    );
    Ok((job, p::BlobIdentity::of(&raw), pin))
}
pub fn snapshot(root: &StableRoot, id: &p::JobId, digest: &p::Digest) -> Result<p::Frame> {
    let lock = root.lock()?;
    let dir = root.directory(OsStr::new(id.as_str()))?;
    let (job, job_record, job_pin) = job(root, &dir, id, digest)?;
    let mut files = Vec::new();
    let mut directories = Vec::new();
    let mut pins = Vec::new();
    let mut dir_pins = Vec::new();
    let mut totals = [0u64; 3];
    let mut counts = [0usize; 3];
    let input = job
        .payload_files()
        .into_iter()
        .map(|f| f.path)
        .collect::<std::collections::BTreeSet<_>>();
    let limits = [
        job.inventory_bounds(false)?,
        job.inventory_bounds(true)?,
        job.auxiliary_bounds()?,
    ];
    let count_bound = file_count_bound(&job)?;
    fn walk<'a>(
        dir: &PinnedDirectory<'a>,
        prefix: &str,
        job: &p::JobManifest,
        input: &std::collections::BTreeSet<p::RelativePath>,
        limits: &[(u64, u64, usize); 3],
        count_bound: usize,
        totals: &mut [u64; 3],
        counts: &mut [usize; 3],
        files: &mut Vec<FetchFile>,
        directories: &mut Vec<p::RelativePath>,
        pins: &mut Vec<PinnedFile>,
        dir_pins: &mut Vec<PinnedDirectory<'a>>,
    ) -> Result<()> {
        for e in std::fs::read_dir(dir.path())? {
            let e = e?;
            let name = e
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("fetch_nonunicode_name"))?;
            let relative = p::RelativePath::new(format!("{prefix}{name}"))?;
            if e.file_type()?.is_dir() {
                ensure!(directories.len() < count_bound, "fetch_directory_count");
                let top = relative.as_str().split('/').next().unwrap();
                ensure!(
                    ["worker", "sources", "build", "inputs", "out", "receipts"].contains(&top),
                    "fetch_unknown_directory"
                );
                let child = dir.directory(OsStr::new(&name))?;
                directories.push(relative.clone());
                walk(
                    &child,
                    &format!("{}/", relative.as_str()),
                    job,
                    input,
                    limits,
                    count_bound,
                    totals,
                    counts,
                    files,
                    directories,
                    pins,
                    dir_pins,
                )?;
                dir_pins.push(child);
            } else {
                let kind = if input.contains(&relative) {
                    0
                } else if relative.as_str().starts_with("out/") {
                    1
                } else {
                    2
                };
                ensure!(counts[kind] < limits[kind].2, "fetch_file_count");
                let bound = if kind < 2 || relative.as_str().starts_with("receipts/") {
                    limits[kind].1
                } else {
                    ensure!(
                        control_name(relative.as_str()),
                        "fetch_path_outside_job_classes"
                    );
                    (p::MAX_RECORD_BYTES as u64).min(limits[2].1)
                };
                let mut pin = dir
                    .pin_file(OsStr::new(&name), bound)?
                    .context("fetch_file_disappeared")?;
                totals[kind] = totals[kind]
                    .checked_add(pin.size())
                    .context("fetch_size_overflow")?;
                ensure!(totals[kind] <= limits[kind].0, "fetch_total_bound");
                counts[kind] += 1;
                let identity = adapter::file_identity(&mut pin)?;
                files.push(FetchFile {
                    entry: p::FileEntry {
                        path: relative,
                        identity,
                    },
                    volume_id: pin.identity().volume_id.clone(),
                    file_id: pin.identity().file_id.clone(),
                });
                pins.push(pin);
            }
        }
        dir.verify()?;
        Ok(())
    }
    walk(
        &dir,
        "",
        &job,
        &input,
        &limits,
        count_bound,
        &mut totals,
        &mut counts,
        &mut files,
        &mut directories,
        &mut pins,
        &mut dir_pins,
    )?;
    files.sort_by(|a, b| a.entry.path.cmp(&b.entry.path));
    directories.sort();
    let state_record = files
        .iter()
        .find(|f| f.entry.path.as_str() == "state.json")
        .map(|f| f.entry.identity.clone());
    let binding = FetchBinding {
        root: adapter::root_identity(root),
        target: target(root, &dir),
        job_id: id.clone(),
        manifest_digest: digest.clone(),
        job_record,
        state_record,
    };
    let snapshot = FetchSnapshot {
        schema: "oev2.nx-fetch-snapshot/1".into(),
        binding: binding.clone(),
        job,
        files,
        directories,
        qualification: "byte_observation_only".into(),
    };
    snapshot.validate()?;
    for pin in &pins {
        pin.verify()?;
    }
    for d in dir_pins {
        d.verify()?;
    }
    job_pin.verify()?;
    dir.verify()?;
    lock.verify()?;
    let payload = serde_json::to_vec(&snapshot)?;
    let response = FetchSnapshotResponse {
        schema: "oev2.nx-fetch-snapshot-response/1".into(),
        binding,
        snapshot_digest: snapshot.digest()?,
        payload: p::BlobIdentity::of(&payload),
    };
    Ok(p::Frame {
        record: serde_json::to_value(response)?,
        payload,
    })
}
pub fn batch(root: &StableRoot, request: &FetchBatch) -> Result<p::Frame> {
    let total = request.validate()?;
    let binding = &request.binding;
    ensure!(
        binding.root == adapter::root_identity(root),
        "fetch_range_root_differs"
    );
    let lock = root.lock()?;
    let dir = root.directory(OsStr::new(binding.job_id.as_str()))?;
    ensure!(
        target(root, &dir) == binding.target,
        "fetch_range_directory_changed"
    );
    let (job, raw_job, job_pin) = job(root, &dir, &binding.job_id, &binding.manifest_digest)?;
    ensure!(
        raw_job == binding.job_record,
        "fetch_range_job_record_changed"
    );
    ensure!(
        total
            <= job
                .capacity
                .as_ref()
                .map_or(p::MAX_PAYLOAD_BYTES, |c| c.0.max_segment_bytes),
        "fetch_range_task_segment_bound"
    );
    let mut state_pin = dir.pin_file(OsStr::new("state.json"), p::MAX_RECORD_BYTES as u64)?;
    let observed_state = state_pin.as_mut().map(adapter::file_identity).transpose()?;
    ensure!(
        observed_state == binding.state_record,
        "fetch_range_state_changed"
    );
    let declared = job
        .payload_files()
        .into_iter()
        .map(|f| f.path)
        .collect::<std::collections::BTreeSet<_>>();
    let input = job.inventory_bounds(false)?.1;
    let output = job.inventory_bounds(true)?.1;
    let auxiliary = job.auxiliary_bounds()?.1;
    let mut pins = Vec::new();
    let mut payload = Vec::with_capacity(total as usize);
    for slice in &request.slices {
        let path = &slice.file.entry.path;
        let bound = if declared.contains(path) {
            input
        } else if path.as_str().starts_with("out/") {
            output
        } else if path.as_str().starts_with("receipts/") {
            auxiliary
        } else {
            ensure!(
                control_name(path.as_str()),
                "fetch_path_outside_job_classes"
            );
            (p::MAX_RECORD_BYTES as u64).min(auxiliary)
        };
        let mut pin = adapter::pin_relative_file(&dir, path, bound)?;
        ensure!(
            pin.identity().volume_id == slice.file.volume_id
                && pin.identity().file_id == slice.file.file_id
                && pin.size() == slice.file.entry.identity.bytes,
            "fetch_range_file_replaced_or_resized"
        );
        // A range is bound to physical identity, size and the current job/state. Full
        // content matching is required at the receiving end and final snapshot.
        let reader = pin.reader()?;
        reader.seek(SeekFrom::Start(slice.offset))?;
        let begin = payload.len();
        payload.resize(begin + slice.bytes as usize, 0);
        reader.read_exact(&mut payload[begin..])?;
        pin.verify()?;
        pins.push(pin);
    }
    for pin in pins {
        pin.verify()?;
    }
    job_pin.verify()?;
    if let Some(p) = state_pin {
        p.verify()?;
    }
    dir.verify()?;
    lock.verify()?;
    let response = FetchBatchResponse {
        schema: "oev2.nx-fetch-batch-response/1".into(),
        request_digest: p::record_digest(request)?,
        payload: p::BlobIdentity::of(&payload),
        whole_file_hash_verified: false,
    };
    response.validate(request, &payload)?;
    Ok(p::Frame {
        record: serde_json::to_value(response)?,
        payload,
    })
}
pub fn range(root: &StableRoot, request: &FetchRange) -> Result<p::Frame> {
    request.validate()?;
    let batch_request = FetchBatch {
        schema: "oev2.nx-fetch-batch/1".into(),
        binding: request.binding.clone(),
        snapshot_digest: request.snapshot_digest.clone(),
        slices: vec![FetchSlice {
            file: request.file.clone(),
            offset: request.offset,
            bytes: request.bytes,
        }],
    };
    let frame = batch(root, &batch_request)?;
    let response = FetchRangeResponse {
        schema: "oev2.nx-fetch-range-response/1".into(),
        request_digest: p::record_digest(request)?,
        payload: p::BlobIdentity::of(&frame.payload),
        whole_file_hash_verified: false,
    };
    response.validate(request, &frame.payload)?;
    Ok(p::Frame {
        record: serde_json::to_value(response)?,
        payload: frame.payload,
    })
}
