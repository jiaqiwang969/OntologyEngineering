//! Bounded pages retain every original event reference in its original order.
use anyhow::{Context, Result, ensure};
use oe_contracts::ArtifactReference;
use oe_nx_protocol as p;
use serde_json::{Value, json};

pub const SCHEMA: &str = "oev2.nx-event-reference-index/1";
pub const PAGE_SCHEMA: &str = "oev2.nx-event-reference-page/1";
const PAGE_COUNT: usize = 256;
const MAX_EVENTS: usize = 65_536;

fn validate(job: &str, events: &[Value]) -> Result<()> {
    p::checked_token(job)?;
    ensure!(events.len() <= MAX_EVENTS, "event_index_count_budget");
    for (i, e) in events.iter().enumerate() {
        let phase = e["phase"].as_str().context("event_index_phase_missing")?;
        p::checked_token(phase)?;
        ensure!(
            e["schema"] == "cad-agent.nx-native-event-reference/v2"
                && e["job_id"] == job
                && e["sequence"].as_u64() == Some(i as u64 + 1),
            "event_index_identity_or_sequence"
        );
        let file: p::FileEntry = serde_json::from_value(e["record"].clone())?;
        ensure!(
            file.path.as_str() == format!("receipts/{:04}-{phase}.json", i + 1)
                && file.identity.bytes > 0
                && file.identity.bytes <= p::MAX_RECORD_BYTES as u64,
            "event_index_source_path_or_size"
        );
        ensure!(
            serde_json::to_vec(e)?.len() <= 4096,
            "event_index_reference_budget"
        );
    }
    Ok(())
}

/// Writer must durably store each complete page before returning its reference.
/// No index is returned when any page write fails; already written pages remain evidence.
pub fn persist(
    job: &str,
    events: &[Value],
    write: &mut dyn FnMut(Value) -> Result<ArtifactReference>,
) -> Result<Value> {
    validate(job, events)?;
    let mut pages = Vec::new();
    for (page, refs) in events.chunks(PAGE_COUNT).enumerate() {
        let value = json!({"schema":PAGE_SCHEMA,"job_id":job,
            "first_sequence":page * PAGE_COUNT + 1,"references":refs});
        ensure!(
            serde_json::to_vec(&value)?.len() <= p::MAX_RECORD_BYTES / 2,
            "event_index_page_budget"
        );
        let reference = write(value)?;
        ensure!(
            event_sequence(&reference, "event_reference_index")?
                == events.len() as u64 + page as u64 + 1,
            "event_index_written_page_sequence"
        );
        pages.push(reference);
    }
    Ok(
        json!({"schema":SCHEMA,"job_id":job,"event_count":events.len(),
        "page_size":PAGE_COUNT,"pages":pages}),
    )
}

/// Check page bytes, contiguous coverage and the original reference identities.
/// Original event bytes still require their own identity checks by the receiver.
pub fn receive(
    job: &str,
    index: &Value,
    read: &mut dyn FnMut(&str) -> Result<Vec<u8>>,
) -> Result<Vec<Value>> {
    ensure!(
        index["schema"] == SCHEMA && index["job_id"] == job && index["page_size"] == PAGE_COUNT,
        "event_index_header"
    );
    let count = index["event_count"]
        .as_u64()
        .context("event_index_count_missing")?;
    ensure!(count <= MAX_EVENTS as u64, "event_index_count_budget");
    let pages: Vec<ArtifactReference> = serde_json::from_value(index["pages"].clone())?;
    ensure!(
        pages.len() == (count as usize).div_ceil(PAGE_COUNT),
        "event_index_page_count"
    );
    p::checked_token(job)?;
    let mut events = Vec::new();
    let mut paths = std::collections::BTreeSet::new();
    for (ordinal, reference) in pages.into_iter().enumerate() {
        ensure!(
            paths.insert(reference.path.clone()),
            "event_index_duplicate_page"
        );
        ensure!(
            event_sequence(&reference, "event_reference_index")? == count + ordinal as u64 + 1,
            "event_index_page_event_sequence"
        );
        let page = receive_event(job, &reference, "event_reference_index", read)?;
        ensure!(
            page["schema"] == PAGE_SCHEMA
                && page["job_id"] == job
                && page["first_sequence"].as_u64() == Some(events.len() as u64 + 1),
            "event_index_page_order"
        );
        let refs = page["references"]
            .as_array()
            .context("event_index_references_missing")?;
        ensure!(
            refs.len() == PAGE_COUNT.min(count as usize - events.len()),
            "event_index_page_coverage"
        );
        events.extend_from_slice(refs);
    }
    ensure!(events.len() == count as usize, "event_index_coverage");
    validate(job, &events)?;
    Ok(events)
}

/// An explicit wrapper is required: consumers must never read omitted large fields as empty.
pub const READBACK_SCHEMA: &str = "oev2.nx-native-indexed-readback/1";
pub const OBSERVATION_SCHEMA: &str = "oev2.nx-native-external-observation/1";
pub const CHUNK_SCHEMA: &str = "oev2.nx-native-field-chunk/1";
pub const CHUNK_BYTES: usize = 128 * 1024;
pub const MAX_EXTERNAL_BYTES: usize = 64 * 1024 * 1024;
pub const LIFECYCLE_RESERVE_BYTES: usize = 16 * 1024;
pub const INLINE_BUDGET: usize = p::MAX_RECORD_BYTES / 2 - LIFECYCLE_RESERVE_BYTES;
const MAX_FIELDS: usize = 256;
const MAX_CHUNKS: usize = MAX_EXTERNAL_BYTES / (CHUNK_BYTES - 3) + 1;

fn field_name(name: &str) -> Result<()> {
    p::checked_token(name)?;
    ensure!(
        !matches!(
            name,
            "external_fields" | "event_reference_index" | "lifecycle"
        ),
        "external_field_reserved_name"
    );
    Ok(())
}

/// Each block is independently durable; a failed write never yields a complete index.
pub fn persist_field(
    job: &str,
    name: &str,
    value: &Value,
    write: &mut dyn FnMut(Value) -> Result<ArtifactReference>,
) -> Result<oe_contracts::NxNativeExternalField> {
    p::checked_token(job)?;
    field_name(name)?;
    let text = serde_json::to_string(value)?;
    ensure!(
        text.len() <= MAX_EXTERNAL_BYTES,
        "external_field_bytes_budget"
    );
    let mut chunks = Vec::new();
    let mut offset = 0;
    while offset < text.len() {
        let mut end = text.len().min(offset + CHUNK_BYTES);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        let chunk = oe_contracts::NxNativeExternalChunk {
            schema: CHUNK_SCHEMA.into(),
            job_id: job.into(),
            name: name.into(),
            ordinal: chunks.len() as u64,
            offset: offset as u64,
            content: text[offset..end].into(),
        };
        let value = serde_json::to_value(chunk)?;
        ensure!(
            serde_json::to_vec(&value)?.len() <= p::MAX_RECORD_BYTES / 2,
            "external_chunk_record_budget"
        );
        let reference = write(value)?;
        event_sequence(&reference, "readback_field_chunk")?;
        chunks.push(reference);
        offset = end;
    }
    Ok(oe_contracts::NxNativeExternalField {
        name: name.into(),
        bytes: text.len() as u64,
        sha256: p::Digest::of(text.as_bytes()).as_str().into(),
        chunks,
    })
}

fn event_sequence(reference: &ArtifactReference, phase: &str) -> Result<u64> {
    p::RelativePath::new(reference.path.clone())?;
    p::Digest::new(reference.sha256.clone())?;
    let sequence = reference
        .path
        .strip_prefix("receipts/")
        .and_then(|s| s.strip_suffix(&format!("-{phase}.json")))
        .and_then(|s| s.parse::<u64>().ok())
        .context("indexed_event_path")?;
    ensure!(
        sequence > 0 && reference.path == format!("receipts/{sequence:04}-{phase}.json"),
        "indexed_event_path_sequence"
    );
    Ok(sequence)
}

/// Verify one original event's identity and return its observation. Callers
/// must separately validate the observation's native/engineering meaning.
/// Shared by indexed transport and the CAD construction evidence consumer.
pub fn receive_event(
    job: &str,
    reference: &ArtifactReference,
    phase: &str,
    read: &mut dyn FnMut(&str) -> Result<Vec<u8>>,
) -> Result<Value> {
    let sequence = event_sequence(reference, phase)?;
    let raw = read(&reference.path)?;
    ensure!(
        raw.len() <= p::MAX_RECORD_BYTES && p::Digest::of(&raw).as_str() == reference.sha256,
        "indexed_event_bytes"
    );
    let event = p::json::strict_json(&raw)?;
    ensure!(
        event["schema"] == "cad-agent.nx-native-event/v2"
            && event["job_id"] == job
            && event["phase"] == phase
            && event["sequence"].as_u64() == Some(sequence),
        "indexed_event_identity"
    );
    Ok(event["observation"].clone())
}

pub fn receive_field(
    job: &str,
    index: &oe_contracts::NxNativeExternalField,
    read: &mut dyn FnMut(&str) -> Result<Vec<u8>>,
) -> Result<Value> {
    p::checked_token(job)?;
    field_name(&index.name)?;
    p::Digest::new(index.sha256.clone())?;
    ensure!(
        index.bytes > 0 && index.bytes <= MAX_EXTERNAL_BYTES as u64,
        "external_field_bytes_budget"
    );
    ensure!(
        !index.chunks.is_empty() && index.chunks.len() <= MAX_CHUNKS,
        "external_field_chunks_budget"
    );
    let mut text = String::new();
    let mut paths = std::collections::BTreeSet::new();
    let mut previous_sequence = None;
    for (ordinal, reference) in index.chunks.iter().enumerate() {
        ensure!(
            paths.insert(&reference.path),
            "external_field_duplicate_chunk"
        );
        let sequence = event_sequence(reference, "readback_field_chunk")?;
        if let Some(previous) = previous_sequence {
            ensure!(sequence == previous + 1, "external_field_event_sequence");
        }
        previous_sequence = Some(sequence);
        let chunk: oe_contracts::NxNativeExternalChunk =
            serde_json::from_value(receive_event(job, reference, "readback_field_chunk", read)?)?;
        ensure!(
            chunk.schema == CHUNK_SCHEMA
                && chunk.job_id == job
                && chunk.name == index.name
                && chunk.ordinal == ordinal as u64
                && chunk.offset == text.len() as u64,
            "external_field_chunk_identity_or_order"
        );
        let remaining = index.bytes as usize - text.len();
        ensure!(
            !chunk.content.is_empty()
                && chunk.content.len() <= CHUNK_BYTES
                && chunk.content.len() <= remaining,
            "external_field_chunk_size"
        );
        if ordinal + 1 < index.chunks.len() {
            ensure!(
                chunk.content.len() >= CHUNK_BYTES - 3,
                "external_field_short_nonfinal_chunk"
            );
        }
        text.push_str(&chunk.content);
    }
    ensure!(
        text.len() as u64 == index.bytes && p::Digest::of(text.as_bytes()).as_str() == index.sha256,
        "external_field_complete_identity"
    );
    // The reconstructed field may exceed the single-record decoder limit. The
    // explicit 64 MiB aggregate budget above is checked before every append.
    Ok(oe_wire::json::strict_json(text.as_bytes())?)
}

/// Externalize fields only after the worker has explicitly selected indexed readback.
/// The caller retains the original outcome; storage never turns failure into success.
pub fn persist_fields(
    job: &str,
    readback: &mut Value,
    write: &mut dyn FnMut(Value) -> Result<ArtifactReference>,
) -> Result<()> {
    let object = readback
        .as_object()
        .context("indexed_readback_not_object")?;
    ensure!(
        !object.contains_key("external_fields"),
        "external_fields_already_present"
    );
    let mut candidates = object
        .iter()
        .filter(|(name, _)| !matches!(name.as_str(), "event_reference_index" | "lifecycle"))
        .map(|(name, value)| Ok((name.clone(), serde_json::to_vec(value)?.len())))
        .collect::<Result<Vec<_>>>()?;
    candidates.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let mut compact = readback.clone();
    let mut fields = Vec::new();
    let mut total = 0usize;
    for (name, bytes) in candidates {
        if !requires_indexing(&compact)? {
            break;
        }
        total = total
            .checked_add(bytes)
            .context("external_fields_bytes_overflow")?;
        ensure!(
            total <= MAX_EXTERNAL_BYTES && fields.len() < MAX_FIELDS,
            "external_fields_aggregate_budget"
        );
        fields.push(persist_field(job, &name, &object[&name], write)?);
        let compact_object = compact.as_object_mut().unwrap();
        compact_object.remove(&name);
        compact_object.insert("external_fields".into(), serde_json::to_value(&fields)?);
    }
    ensure!(
        !requires_indexing(&compact)?,
        "indexed_readback_remaining_budget"
    );
    *readback = compact;
    Ok(())
}

/// Selection reserves the lifecycle which becomes known after native termination.
/// Even a result just below 2 MiB must be indexed when the final envelope could grow.
pub fn requires_indexing(readback: &Value) -> Result<bool> {
    Ok(serde_json::to_vec(readback)?.len() > INLINE_BUDGET)
}

pub fn finish_lifecycle(readback: &mut Value, lifecycle: Value) -> Result<()> {
    ensure!(
        serde_json::to_vec(&lifecycle)?.len() + 32 <= LIFECYCLE_RESERVE_BYTES,
        "native_lifecycle_reserved_budget"
    );
    let object = readback
        .as_object_mut()
        .context("native_readback_not_object")?;
    ensure!(
        !object.contains_key("lifecycle"),
        "native_lifecycle_already_present"
    );
    object.insert("lifecycle".into(), lifecycle);
    ensure!(
        serde_json::to_vec(readback)?.len() <= p::MAX_RECORD_BYTES / 2,
        "native_final_readback_budget"
    );
    Ok(())
}

/// Reconstruct every external field, rejecting shadowing and aggregate over-budget input.
/// Event reference pages are received independently with `receive` below.
pub fn receive_fields(
    job: &str,
    readback: &Value,
    read: &mut dyn FnMut(&str) -> Result<Vec<u8>>,
) -> Result<Value> {
    p::checked_token(job)?;
    ensure!(
        serde_json::to_vec(readback)?.len() <= p::MAX_RECORD_BYTES / 2,
        "indexed_readback_budget"
    );
    let mut result = readback.clone();
    let object = result
        .as_object_mut()
        .context("indexed_readback_not_object")?;
    let Some(raw) = object.remove("external_fields") else {
        return Ok(result);
    };
    let fields: Vec<oe_contracts::NxNativeExternalField> = serde_json::from_value(raw)?;
    ensure!(
        !fields.is_empty() && fields.len() <= MAX_FIELDS,
        "external_fields_count_budget"
    );
    let mut total = 0u64;
    for field in fields {
        total = total
            .checked_add(field.bytes)
            .context("external_fields_bytes_overflow")?;
        ensure!(
            total <= MAX_EXTERNAL_BYTES as u64,
            "external_fields_aggregate_budget"
        );
        ensure!(
            !object.contains_key(&field.name),
            "external_fields_duplicate_or_shadowing"
        );
        object.insert(field.name.clone(), receive_field(job, &field, read)?);
    }
    Ok(result)
}

pub fn receive_observation(
    job: &str,
    observation: &Value,
    read: &mut dyn FnMut(&str) -> Result<Vec<u8>>,
) -> Result<Value> {
    ensure!(
        observation["schema"] == OBSERVATION_SCHEMA && observation["job_id"] == job,
        "external_observation_header"
    );
    let index: oe_contracts::NxNativeExternalField =
        serde_json::from_value(observation["field"].clone())?;
    ensure!(
        index.name == "observation",
        "external_observation_field_name"
    );
    receive_field(job, &index, read)
}

/// Receive an explicitly indexed native readback and verify all original event bytes.
/// This reconstructs the original fields; callers must still validate NativeResult's
/// manifest/process/outcome binding and the engineering meaning of each observation.
pub fn receive_readback(
    job: &str,
    schema: &str,
    readback: &Value,
    read: &mut dyn FnMut(&str) -> Result<Vec<u8>>,
) -> Result<Value> {
    ensure!(schema == READBACK_SCHEMA, "indexed_readback_schema");
    ensure!(
        serde_json::to_vec(readback)?.len() <= p::MAX_RECORD_BYTES / 2,
        "indexed_readback_budget"
    );
    // The worker writes all field blocks directly after the original event-index
    // pages. Bind that continuation as well as each field's local block order.
    let count = readback["event_reference_index"]["event_count"]
        .as_u64()
        .context("indexed_readback_count_missing")?;
    ensure!(count <= MAX_EVENTS as u64, "event_index_count_budget");
    let mut sequence = count + count.div_ceil(PAGE_COUNT as u64) + 1;
    if let Some(raw) = readback.get("external_fields") {
        let fields: Vec<oe_contracts::NxNativeExternalField> = serde_json::from_value(raw.clone())?;
        ensure!(fields.len() <= MAX_FIELDS, "external_fields_count_budget");
        for field in fields {
            ensure!(
                field.chunks.len() <= MAX_CHUNKS,
                "external_field_chunks_budget"
            );
            for reference in field.chunks {
                ensure!(
                    event_sequence(&reference, "readback_field_chunk")? == sequence,
                    "indexed_readback_field_sequence"
                );
                sequence += 1;
            }
        }
    }
    let mut restored = receive_fields(job, readback, read)?;
    let object = restored
        .as_object_mut()
        .context("indexed_readback_not_object")?;
    let index = object
        .remove("event_reference_index")
        .context("indexed_readback_index_missing")?;
    ensure!(
        !object.contains_key("load_status_records") && !object.contains_key("partial_observations"),
        "indexed_readback_diagnostic_shadowing"
    );
    let references = receive(job, &index, read)?;
    for reference in &references {
        let file: p::FileEntry = serde_json::from_value(reference["record"].clone())?;
        let phase = reference["phase"]
            .as_str()
            .context("indexed_readback_phase_missing")?;
        let artifact = ArtifactReference {
            path: file.path.as_str().into(),
            sha256: file.identity.sha256.as_str().into(),
        };
        let mut checked_read = |path: &str| {
            let raw = read(path)?;
            ensure!(
                raw.len() as u64 == file.identity.bytes,
                "indexed_readback_original_size"
            );
            Ok(raw)
        };
        let observation = receive_event(job, &artifact, phase, &mut checked_read)?;
        if observation["schema"] == OBSERVATION_SCHEMA {
            receive_observation(job, &observation, read)?;
        }
    }
    object.insert(
        "load_status_records".into(),
        json!(
            references
                .iter()
                .filter(|e| e["phase"] == "native_load_status")
                .collect::<Vec<_>>()
        ),
    );
    if object
        .get("partial_observations_form")
        .and_then(Value::as_str)
        == Some("complete_event_reference_index")
    {
        object.insert("partial_observations".into(), json!(references));
        object.insert(
            "partial_observations_form".into(),
            json!("source_bound_event_references"),
        );
    }
    Ok(restored)
}
