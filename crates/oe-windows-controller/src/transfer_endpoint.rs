//! Dedicated transfer-root endpoint. It never stages or dispatches an NX job.
use crate::{
    adapter,
    transfer_store::{Session, Store},
    transfer_store_windows::PinnedStore,
    windows::*,
};
use anyhow::{Context, Result, ensure};
use oe_nx_protocol::{
    self as p,
    segmented::TransferPlan,
    transfer::{self, TransferResponse},
};
use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
const MARKER: &str = ".oe-transfer-root.json";
#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Purpose {
    schema: String,
    root: p::RootIdentity,
}
pub fn ensure_native_root(root: &StableRoot) -> Result<()> {
    let dir = root.as_directory()?;
    ensure!(
        dir.pin_file(OsStr::new(MARKER), p::MAX_RECORD_BYTES as u64)?
            .is_none(),
        "transfer_root_cannot_admit_native_job"
    );
    Ok(())
}
pub fn execute(root: &StableRoot, request: &p::Request, body: &[u8]) -> Result<TransferResponse> {
    request.validate()?;
    ensure!(transfer::is_transfer(request), "not_transfer_request");
    let (job_id, binding) = transfer::binding(request)?;
    let incoming = if let p::Request::TransferBegin { plan_record } = request {
        Some(transfer::plan(plan_record)?)
    } else {
        None
    };
    if let Some(plan) = &incoming {
        ensure!(
            plan.root == adapter::root_identity(root),
            "transfer_begin_root_differs"
        );
    }
    if let p::Request::TransferPut { payload, .. } | p::Request::TransferPutBatch { payload, .. } =
        request
    {
        payload.verify(body)?;
    } else {
        ensure!(body.is_empty(), "transfer_unexpected_payload");
    }
    let lock = root.lock()?;
    let top = root.as_directory()?;
    let purpose = Purpose {
        schema: "oev2.nx-transfer-root/1".into(),
        root: adapter::root_identity(root),
    };
    let marker = top.pin_file(OsStr::new(MARKER), p::MAX_RECORD_BYTES as u64)?;
    let mut marker = match marker {
        Some(pin) => pin,
        None => {
            ensure!(incoming.is_some(), "transfer_root_not_initialized");
            for entry in std::fs::read_dir(root.path())? {
                ensure!(
                    entry?.file_name() == OsStr::new(LOCK_NAME),
                    "transfer_begin_requires_empty_dedicated_root"
                );
            }
            top.create_file_new(
                OsStr::new(MARKER),
                &serde_json::to_vec(&purpose)?,
                p::MAX_RECORD_BYTES as u64,
            )?;
            top.pin_file(OsStr::new(MARKER), p::MAX_RECORD_BYTES as u64)?
                .context("transfer_root_marker_missing")?
        }
    };
    let actual: Purpose = p::decode(&marker.read_all(p::MAX_RECORD_BYTES as u64)?)?;
    ensure!(actual == purpose, "transfer_root_marker_differs");
    let directory = if incoming.is_some() {
        root.create_directory_new(OsStr::new(job_id.as_str()))?
    } else {
        root.directory(OsStr::new(job_id.as_str()))?
    };
    let mut store = PinnedStore::new(&directory, &lock)?;
    let plan: TransferPlan = match incoming {
        Some(p) => p,
        None => p::decode(&store.read("plan.json", p::MAX_RECORD_BYTES as u64)?)?,
    };
    ensure!(
        plan.job_id == job_id
            && plan.root == purpose.root
            && plan.binding()? == binding
            && plan.direction == "upload",
        "transfer_stored_plan_differs"
    );
    let count = u32::try_from(plan.spans.len())?;
    if let p::Request::TransferPublish {
        output_id,
        publication_id,
        expected_target,
        ..
    }
    | p::Request::TransferInspectPublication {
        output_id,
        publication_id,
        expected_target,
        ..
    }
    | p::Request::TransferRecoverPublication {
        output_id,
        publication_id,
        expected_target,
        ..
    } = request
    {
        Session::open_existing(&mut store, plan.clone())?.finish()?;
        use crate::transfer_publication::{Operation, publish};
        let op = match request {
            p::Request::TransferPublish { .. } => Operation::Publish,
            p::Request::TransferRecoverPublication { .. } => Operation::Recover,
            _ => Operation::Inspect,
        };
        let published = publish(
            root,
            &lock,
            &plan,
            output_id,
            publication_id,
            expected_target,
            op,
        )?;
        store.verify()?;
        marker.verify()?;
        lock.verify()?;
        let response = TransferResponse {
            schema: transfer::RESPONSE_SCHEMA.into(),
            root: purpose.root,
            job_id,
            plan_digest: binding,
            next_index: count,
            total_chunks: count,
            status: "published_source".into(),
            verified_bytes: Some(published.bytes),
            native_execution_started: false,
            reconstructed: None,
            reconstruction_output: None,
            publication: Some(published),
        };
        response.validate(request, &adapter::root_identity(root))?;
        return Ok(response);
    }
    if let p::Request::TransferReconstruct { output_id, .. } = request {
        crate::transfer_reconstruct::validate_plan(&plan)?;
        Session::open_existing(&mut store, plan.clone())?.finish()?;
        let output = root.create_directory_new(OsStr::new(output_id.as_str()))?;
        let mut target = crate::transfer_reconstruct_windows::PinnedTarget::new(&output, &lock)?;
        let tree = crate::transfer_reconstruct::reconstruct(&mut store, plan, &mut target)?;
        store.verify()?;
        marker.verify()?;
        lock.verify()?;
        let response = TransferResponse {
            schema: transfer::RESPONSE_SCHEMA.into(),
            root: purpose.root,
            job_id,
            plan_digest: binding,
            next_index: count,
            total_chunks: count,
            status: "reconstructed_files".into(),
            verified_bytes: Some(tree.bytes),
            native_execution_started: false,
            publication: None,
            reconstructed: Some(tree),
            reconstruction_output: Some(output_id.clone()),
        };
        response.validate(request, &adapter::root_identity(root))?;
        return Ok(response);
    }
    let mut session = Session::open(
        &mut store,
        plan,
        matches!(request, p::Request::TransferCommit { .. }),
    )?;
    if let p::Request::TransferPut { index, .. } = request {
        session.receive(&binding, *index as usize, body)?;
    }
    if let p::Request::TransferPutBatch { index, count, .. } = request {
        session.receive_batch(&binding, *index as usize, *count as usize, body)?;
    }
    let next = u32::try_from(session.next_index())?;
    let verified = if matches!(request, p::Request::TransferVerify { .. }) {
        Some(session.finish()?.bytes)
    } else {
        drop(session);
        None
    };
    store.verify()?;
    marker.verify()?;
    lock.verify()?;
    let response = TransferResponse {
        schema: transfer::RESPONSE_SCHEMA.into(),
        root: purpose.root,
        job_id,
        plan_digest: binding,
        next_index: next,
        total_chunks: count,
        status: if verified.is_some() {
            "verified_chunks"
        } else {
            "ready"
        }
        .into(),
        verified_bytes: verified,
        native_execution_started: false,
        publication: None,
        reconstructed: None,
        reconstruction_output: None,
    };
    response.validate(request, &adapter::root_identity(root))?;
    Ok(response)
}

/// Observe an already initialized transfer-only root; never create a marker here.
pub fn pin_transfer_root(root: &StableRoot) -> Result<PinnedFile> {
    let top = root.as_directory()?;
    let mut pin = top
        .pin_file(OsStr::new(MARKER), p::MAX_RECORD_BYTES as u64)?
        .context("published_source_transfer_root_missing")?;
    let actual: Purpose = p::decode(&pin.read_all(p::MAX_RECORD_BYTES as u64)?)?;
    ensure!(
        actual
            == Purpose {
                schema: "oev2.nx-transfer-root/1".into(),
                root: adapter::root_identity(root)
            },
        "published_source_transfer_root_differs"
    );
    pin.verify()?;
    Ok(pin)
}
