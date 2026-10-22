//! Full published-source staging under the caller's admitted native root lock.
use crate::{
    adapter,
    native_input_copy::Source,
    native_input_copy_windows::PublishedSource,
    native_stage, transfer_endpoint,
    transfer_reconstruct::Target,
    transfer_reconstruct_windows::PinnedTarget,
    windows::{PinnedFile, StableRoot, WriterLock},
};
use anyhow::{Context, Result, ensure};
use oe_nx_protocol::{
    self as p,
    published_payload::PreparedPayload,
    published_stage::{PublishedStageRequest, PublishedStageResponse, StageOperation},
    segmented::TransferPlan,
};
use std::{ffi::OsStr, path::Path};
pub fn execute(
    root: &StableRoot,
    lock: &WriterLock<'_>,
    request: &PublishedStageRequest,
) -> Result<PublishedStageResponse> {
    request.validate()?;
    transfer_endpoint::ensure_native_root(root)?;
    ensure!(
        adapter::root_identity(root) == request.job.root
            && adapter::root_identity(lock.root()) == request.job.root,
        "published_stage_native_root_differs"
    );
    lock.verify()?;
    let mut roots = Vec::<StableRoot>::new();
    let mut indices = Vec::new();
    for (path, source) in request.source_root_paths.iter().zip(&request.plan.sources) {
        let observed = StableRoot::open(Path::new(path), None)?;
        let id = adapter::root_identity(&observed);
        ensure!(
            id == source.publication.source_root && id != request.job.root,
            "published_stage_source_root_differs"
        );
        let index = if let Some(i) = roots.iter().position(|r| adapter::root_identity(r) == id) {
            i
        } else {
            roots.push(observed);
            roots.len() - 1
        };
        indices.push(index);
    }
    // Locks are nonblocking. Any contention fails before a native directory write.
    let locks = roots
        .iter()
        .map(StableRoot::lock)
        .collect::<std::io::Result<Vec<_>>>()?;
    let markers = roots
        .iter()
        .map(transfer_endpoint::pin_transfer_root)
        .collect::<Result<Vec<_>>>()?;
    let mut plans = Vec::<TransferPlan>::new();
    let mut plan_pins = Vec::<PinnedFile>::new();
    let mut outputs = Vec::new();
    let mut metadata = Vec::new();
    for (i, source) in request.plan.sources.iter().enumerate() {
        let root = &roots[indices[i]];
        let stored = root.directory(OsStr::new(source.publication.job_id.as_str()))?;
        let mut pin = stored
            .pin_file(OsStr::new("plan.json"), p::MAX_RECORD_BYTES as u64)?
            .context("published_stage_source_plan_missing")?;
        let plan: TransferPlan = p::decode(&pin.read_all(p::MAX_RECORD_BYTES as u64)?)?;
        ensure!(
            plan.root == source.publication.source_root
                && plan.job_id == source.publication.job_id
                && plan.direction == "upload"
                && plan.binding()? == source.publication.plan_digest
                && plan.files == source.files,
            "published_stage_source_plan_differs"
        );
        pin.verify()?;
        plan_pins.push(pin);
        plans.push(plan);
        outputs.push(root.directory(OsStr::new(source.publication.output_id.as_str()))?);
        metadata.push(root.directory(OsStr::new(source.publication.publication_id.as_str()))?);
    }
    let mut sources = Vec::new();
    for (i, source) in request.plan.sources.iter().enumerate() {
        let index = indices[i];
        sources.push(PublishedSource::open(
            &roots[index],
            &locks[index],
            &outputs[i],
            &metadata[i],
            &plans[i],
            &source.publication,
        )?);
    }
    let (dir, fresh) = match root.directory(OsStr::new(request.job.job_id.as_str())) {
        Ok(d) => (d, false),
        Err(e)
            if request.operation == StageOperation::Prepare
                && matches!(e.raw_os_error(), Some(2 | 3)) =>
        {
            (
                root.create_directory_new(OsStr::new(request.job.job_id.as_str()))?,
                true,
            )
        }
        Err(e) => return Err(e.into()),
    };
    let prepared = if fresh {
        let mut target = PinnedTarget::new(&dir, lock)?;
        let mut borrowed = sources
            .iter_mut()
            .map(|s| s as &mut dyn Source)
            .collect::<Vec<_>>();
        crate::native_payload_copy::prepare(
            &request.job,
            &request.plan,
            &mut borrowed,
            &mut target,
        )?
    } else {
        let target = PinnedTarget::new(&dir, lock)?;
        PreparedPayload::for_target(&request.job, &request.plan, target.identity()?)?
    };
    let operation = match request.operation {
        StageOperation::Prepare => native_stage::Operation::Commit,
        StageOperation::Inspect => native_stage::Operation::Inspect,
        StageOperation::Recover => native_stage::Operation::Recover,
    };
    let state = native_stage::stage(
        &dir,
        lock,
        &request.job,
        &request.plan,
        &prepared,
        operation,
    )?;
    for source in &sources {
        source.verify()?;
    }
    for pin in markers.iter().chain(plan_pins.iter()) {
        pin.verify()?;
    }
    for l in &locks {
        l.verify()?;
    }
    lock.verify()?;
    let response = PublishedStageResponse {
        schema: "oev2.nx-published-stage-response/1".into(),
        request_digest: p::record_digest(request)?,
        root: request.job.root.clone(),
        prepared,
        state,
        native_execution_started: false,
    };
    response.validate(request, &adapter::root_identity(root))?;
    Ok(response)
}
