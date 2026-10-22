//! Copies the complete executable/source/parameter/input closure from publications.
//! A complete prepared payload still needs an explicit atomic stage transition.
use crate::native_input_copy::{Destination, Source, stream_checked};
use anyhow::{Result, ensure};
use oe_nx_protocol::{
    published_payload::{PreparedPayload, PublishedPayloadPlan},
    *,
};
pub fn prepare<T: Destination>(
    job: &JobManifest,
    plan: &PublishedPayloadPlan,
    sources: &mut [&mut dyn Source],
    target: &mut T,
) -> Result<PreparedPayload> {
    plan.validate(job)?;
    ensure!(
        sources.len() == plan.sources.len(),
        "payload_source_count_differs"
    );
    for (actual, expected) in sources.iter().zip(&plan.sources) {
        ensure!(
            actual.publication() == &expected.publication && actual.files() == expected.files,
            "payload_actual_source_differs"
        );
        actual.verify()?;
    }
    ensure!(
        target.root_identity()? == job.root,
        "payload_destination_root_differs"
    );
    let identity = target.identity()?;
    let mut payload = job.payload_files();
    payload.sort_by(|a, b| a.path.cmp(&b.path));
    let receipt = PreparedPayload::for_target(job, plan, identity.clone())?;
    let segment = job.capacity.as_ref().unwrap().0.max_segment_bytes;
    // Parse the actual bound parameters BEFORE any destination write. The checked
    // writer refuses oversize before extending memory, even for a bad source.
    ensure!(
        job.params.identity.bytes <= MAX_RECORD_BYTES as u64,
        "payload_parameter_record_bound"
    );
    let mut parameters = None;
    for (i, binding) in plan.sources.iter().enumerate() {
        for file in &binding.files {
            if binding.mount.map(file)?.path == job.params.path {
                let mut raw = Vec::new();
                stream_checked(sources[i], file, segment, &mut raw)?;
                job.validate_parameters(&raw)?;
                parameters = Some(raw);
            }
        }
    }
    ensure!(parameters.is_some(), "payload_parameters_missing");
    for source in sources.iter() {
        source.verify()?;
    }
    target.verify_empty()?;
    // Intent originals precede payload writes, so an incomplete tree retains its
    // exact requested job and source identities; it is not executable state.
    let mut all = payload.clone();
    for (name, raw) in [
        ("job.json", serde_json::to_vec(job)?),
        ("source-plan.json", serde_json::to_vec(plan)?),
    ] {
        ensure!(
            raw.len() <= MAX_RECORD_BYTES,
            "payload_control_record_bound"
        );
        let file = FileEntry::from_bytes(name, &raw)?;
        target.write_new(&file, |out| {
            out.write_all(&raw)?;
            Ok(())
        })?;
        ensure!(
            target.readback(&file)? == file.identity,
            "payload_control_readback_differs"
        );
        all.push(file);
    }
    for (i, binding) in plan.sources.iter().enumerate() {
        for file in &binding.files {
            let mapped = binding.mount.map(file)?;
            target.write_new(&mapped, |out| {
                stream_checked(sources[i], file, segment, out)
            })?;
            ensure!(
                target.readback(&mapped)? == mapped.identity,
                "payload_file_readback_differs"
            );
        }
    }
    for source in sources.iter() {
        source.verify()?;
    }
    target.verify_complete(&all)?;
    ensure!(
        target.identity()? == identity && target.root_identity()? == job.root,
        "payload_target_changed"
    );
    receipt.validate(job, plan)?;
    Ok(receipt)
}
