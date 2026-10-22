use oe_contracts::ArtifactReference;
use oe_nx_protocol as p;
use oe_nx_worker::event_index;
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn references() -> Vec<Value> {
    (1..=4546)
        .map(|i| {
            json!({"schema":"cad-agent.nx-native-event-reference/v2",
        "job_id":"large-native","sequence":i,"phase":"native_load_status",
        "record":{"path":format!("receipts/{i:04}-native_load_status.json"),
        "identity":p::BlobIdentity::of(b"original")}})
        })
        .collect()
}

#[test]
fn event_index_preserves_large_failure_and_rejects_loss_or_tampering() {
    let events = references();
    let mut files = BTreeMap::new();
    let index=event_index::persist("large-native",&events,&mut |page| {
        let sequence = events.len() + files.len() + 1;
        let path=format!("receipts/{sequence:04}-event_reference_index.json");
        let bytes=serde_json::to_vec(&json!({"schema":"cad-agent.nx-native-event/v2","sequence":sequence,"job_id":"large-native",
            "phase":"event_reference_index","observation":page}))?;
        let sha256=p::Digest::of(&bytes).as_str().to_owned();files.insert(path.clone(),bytes);
        Ok(ArtifactReference{path,sha256})
    }).unwrap();
    assert_eq!(files.len(), 18);
    assert!(serde_json::to_vec(&index).unwrap().len() < 16 * 1024);
    let restored =
        event_index::receive("large-native", &index, &mut |p| Ok(files[p].clone())).unwrap();
    assert_eq!(events, restored);
    let mut missing = index.clone();
    missing["pages"].as_array_mut().unwrap().pop();
    assert!(event_index::receive("large-native", &missing, &mut |p| Ok(files[p].clone())).is_err());
    let mut duplicate = index.clone();
    duplicate["pages"][1] = duplicate["pages"][0].clone();
    assert!(
        event_index::receive("large-native", &duplicate, &mut |p| Ok(files[p].clone())).is_err()
    );
    assert!(
        event_index::receive("large-native", &index, &mut |p| {
            let mut b = files[p].clone();
            b.push(b' ');
            Ok(b)
        })
        .is_err()
    );
    assert!(event_index::receive("other-job", &index, &mut |p| Ok(files[p].clone())).is_err());
    let mut gap = events.clone();
    gap.remove(3);
    assert!(
        event_index::persist("large-native", &gap, &mut |_| panic!(
            "invalid input wrote page"
        ))
        .is_err()
    );
    let mut calls = 0;
    assert!(
        event_index::persist("large-native", &events, &mut |_| {
            calls += 1;
            anyhow::bail!("disk write failed")
        })
        .is_err()
    );
    assert_eq!(calls, 1);
}

fn write_chunk(
    files: &mut BTreeMap<String, Vec<u8>>,
    observation: Value,
) -> anyhow::Result<ArtifactReference> {
    let sequence = files.len() + 1;
    let path = format!("receipts/{sequence:04}-readback_field_chunk.json");
    let raw = serde_json::to_vec(&json!({"schema":"cad-agent.nx-native-event/v2",
        "job_id":"large-native","sequence":sequence,"phase":"readback_field_chunk","observation":observation}))?;
    assert!(raw.len() <= p::MAX_RECORD_BYTES);
    let reference = ArtifactReference {
        path: path.clone(),
        sha256: p::Digest::of(&raw).as_str().into(),
    };
    files.insert(path, raw);
    Ok(reference)
}

fn rewrite_chunk(
    files: &mut BTreeMap<String, Vec<u8>>,
    reference: &mut ArtifactReference,
    change: impl FnOnce(&mut Value),
) {
    let mut event: Value = serde_json::from_slice(&files[&reference.path]).unwrap();
    change(&mut event);
    let raw = serde_json::to_vec(&event).unwrap();
    reference.sha256 = p::Digest::of(&raw).as_str().into();
    files.insert(reference.path.clone(), raw);
}

#[test]
fn external_readback_fields_roundtrip_and_reject_incomplete_or_forged_records() {
    let assembly = json!({"occurrences": (0..1514).map(|i| json!({"id":format!("E_{i:032X}"),
        "translation_m":[i as f64 * 0.001,0.0,0.0],"prototype":format!("part-{i}.prt"),
        "source_note":"机械零件🛠\n\"\\".repeat(120)})).collect::<Vec<_>>()});
    let before = json!({"action":"inspect_assembly","assembly":assembly,
        "save_requested":false,"engineering_adoption":"not_assessed"});
    assert!(serde_json::to_vec(&before).unwrap().len() > p::MAX_RECORD_BYTES / 2);
    let mut readback = before.clone();
    let mut files = BTreeMap::new();
    event_index::persist_fields("large-native", &mut readback, &mut |chunk| {
        write_chunk(&mut files, chunk)
    })
    .unwrap();
    assert!(readback.get("assembly").is_none());
    assert!(serde_json::to_vec(&readback).unwrap().len() < 32 * 1024);
    assert_eq!(
        event_index::receive_fields("large-native", &readback, &mut |p| Ok(files[p].clone()))
            .unwrap(),
        before
    );
    let original: oe_contracts::NxNativeExternalField =
        serde_json::from_value(readback["external_fields"][0].clone()).unwrap();
    let mut wrong = original.clone();
    wrong.chunks.pop();
    assert!(
        event_index::receive_field("large-native", &wrong, &mut |p| Ok(files[p].clone())).is_err()
    );
    let mut wrong = original.clone();
    wrong.chunks[1] = wrong.chunks[0].clone();
    assert!(
        event_index::receive_field("large-native", &wrong, &mut |p| Ok(files[p].clone())).is_err()
    );
    for change in 0..8 {
        let mut mutated = files.clone();
        let mut wrong = original.clone();
        rewrite_chunk(&mut mutated, &mut wrong.chunks[0], |e| match change {
            0 => e["schema"] = json!("another-event/1"),
            1 => e["sequence"] = json!(900),
            2 => e["observation"]["name"] = json!("another_field"),
            3 => e["observation"]["job_id"] = json!("other-job"),
            4 => e["observation"]["ordinal"] = json!(1),
            5 => e["observation"]["offset"] = json!(1),
            6 => e["observation"]["content"] = json!("short"),
            _ => e["observation"]["extra"] = json!("unrecognized"),
        });
        assert!(
            event_index::receive_field("large-native", &wrong, &mut |p| Ok(mutated[p].clone()))
                .is_err(),
            "mutation {change}"
        );
    }
    let mut wrong = original.clone();
    wrong.bytes += 1;
    assert!(
        event_index::receive_field("large-native", &wrong, &mut |p| Ok(files[p].clone())).is_err()
    );
    let mut wrong = original.clone();
    wrong.sha256 = "0".repeat(64);
    assert!(
        event_index::receive_field("large-native", &wrong, &mut |p| Ok(files[p].clone())).is_err()
    );
    let mut wrong = original.clone();
    wrong.bytes = event_index::MAX_EXTERNAL_BYTES as u64 + 1;
    assert!(
        event_index::receive_field("large-native", &wrong, &mut |_| panic!("over-budget read"))
            .is_err()
    );
    assert!(
        event_index::receive_field("large-native", &original, &mut |p| {
            let mut b = files[p].clone();
            b.push(b' ');
            Ok(b)
        })
        .is_err()
    );
    let mut shadow = readback.clone();
    shadow["assembly"] = json!({});
    assert!(
        event_index::receive_fields("large-native", &shadow, &mut |p| Ok(files[p].clone()))
            .is_err()
    );
    let mut duplicate = readback.clone();
    duplicate["external_fields"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::to_value(&original).unwrap());
    assert!(
        event_index::receive_fields("large-native", &duplicate, &mut |p| Ok(files[p].clone()))
            .is_err()
    );
    assert!(
        event_index::persist_field(
            "large-native",
            "assembly",
            &assembly,
            &mut |_| anyhow::bail!("disk-full")
        )
        .is_err()
    );
    let observation =
        json!({"schema":event_index::OBSERVATION_SCHEMA,"job_id":"large-native","field":original});
    assert!(
        event_index::receive_observation("large-native", &observation, &mut |p| Ok(
            files[p].clone()
        ))
        .is_err()
    );
    let observation_field =
        event_index::persist_field("large-native", "observation", &assembly, &mut |c| {
            write_chunk(&mut files, c)
        })
        .unwrap();
    let observation = json!({"schema":event_index::OBSERVATION_SCHEMA,"job_id":"large-native","field":observation_field});
    assert_eq!(
        event_index::receive_observation("large-native", &observation, &mut |p| Ok(
            files[p].clone()
        ))
        .unwrap(),
        assembly
    );
}

#[test]
fn event_page_wrapper_is_bound_even_when_its_hash_is_updated() {
    let events = references();
    let mut files = BTreeMap::new();
    let index = event_index::persist("large-native", &events, &mut |page| {
        let sequence = events.len() + files.len() + 1;
        let path = format!("receipts/{sequence:04}-event_reference_index.json");
        let raw = serde_json::to_vec(
            &json!({"schema":"cad-agent.nx-native-event/v2","job_id":"large-native",
            "sequence":sequence,"phase":"event_reference_index","observation":page}),
        )?;
        let reference = ArtifactReference {
            path: path.clone(),
            sha256: p::Digest::of(&raw).as_str().into(),
        };
        files.insert(path, raw);
        Ok(reference)
    })
    .unwrap();
    for key in ["schema", "sequence"] {
        let mut changed = index.clone();
        let mut changed_files = files.clone();
        let mut reference: ArtifactReference =
            serde_json::from_value(changed["pages"][0].clone()).unwrap();
        rewrite_chunk(&mut changed_files, &mut reference, |e| {
            e[key] = json!("wrong")
        });
        changed["pages"][0] = serde_json::to_value(reference).unwrap();
        assert!(
            event_index::receive("large-native", &changed, &mut |p| Ok(
                changed_files[p].clone()
            ))
            .is_err()
        );
    }
}

#[test]
fn aggregate_small_fields_and_final_lifecycle_keep_the_original_limit() {
    let mut many = json!({});
    for i in 0..10 {
        many[format!("field_{i}")] = json!("x".repeat(230 * 1024));
    }
    let mut boundary = json!({"assembly":"x".repeat(p::MAX_RECORD_BYTES/2-100)});
    assert!(serde_json::to_vec(&boundary).unwrap().len() < p::MAX_RECORD_BYTES / 2);
    assert!(event_index::requires_indexing(&boundary).unwrap());
    for readback in [&mut many, &mut boundary] {
        let before = readback.clone();
        let mut files = BTreeMap::new();
        event_index::persist_fields("large-native", readback, &mut |c| {
            write_chunk(&mut files, c)
        })
        .unwrap();
        assert!(!event_index::requires_indexing(readback).unwrap());
        assert_eq!(
            event_index::receive_fields("large-native", readback, &mut |p| Ok(files[p].clone()))
                .unwrap(),
            before
        );
        event_index::finish_lifecycle(
            readback,
            json!({"terminate_attempted":true,"terminate_return":0,
            "runtime_scope":"explicitly_pinned_libraries_only","padding":"x".repeat(8192)}),
        )
        .unwrap();
        assert!(serde_json::to_vec(readback).unwrap().len() <= p::MAX_RECORD_BYTES / 2);
    }
    let mut small = json!({"action":"inspect"});
    assert!(!event_index::requires_indexing(&small).unwrap());
    assert!(
        event_index::finish_lifecycle(
            &mut small,
            json!("x".repeat(event_index::LIFECYCLE_RESERVE_BYTES))
        )
        .is_err()
    );
}

#[test]
fn reconstructed_json_preserves_marker_objects_and_refuses_duplicate_keys() {
    let marker =
        oe_wire::json::strict_json(br#"{"$serde_json::private::Number":"1e400"}"#).unwrap();
    let mut files = BTreeMap::new();
    let index = event_index::persist_field("large-native", "assembly", &marker, &mut |c| {
        write_chunk(&mut files, c)
    })
    .unwrap();
    assert_eq!(
        event_index::receive_field("large-native", &index, &mut |p| Ok(files[p].clone())).unwrap(),
        marker
    );
    let mut wrong = index.clone();
    let duplicate = r#"{"part":1,"part":2}"#;
    rewrite_chunk(&mut files, &mut wrong.chunks[0], |e| {
        e["observation"]["content"] = json!(duplicate)
    });
    wrong.bytes = duplicate.len() as u64;
    wrong.sha256 = p::Digest::of(duplicate.as_bytes()).as_str().into();
    assert!(
        event_index::receive_field("large-native", &wrong, &mut |p| Ok(files[p].clone())).is_err()
    );
}

#[test]
fn complete_receiver_restores_fields_and_checks_original_events() {
    let mut files = BTreeMap::new();
    let mut references = Vec::new();
    for sequence in 1..=3 {
        let phase = if sequence == 2 {
            "native_load_status"
        } else {
            "original_operation"
        };
        let path = format!("receipts/{sequence:04}-{phase}.json");
        let event = json!({"schema":"cad-agent.nx-native-event/v2","job_id":"large-native",
            "sequence":sequence,"phase":phase,"observation":{"return_code":0}});
        let raw = serde_json::to_vec(&event).unwrap();
        references.push(json!({"schema":"cad-agent.nx-native-event-reference/v2","job_id":"large-native",
            "sequence":sequence,"phase":phase,"record":{"path":path,"identity":p::BlobIdentity::of(&raw)}}));
        files.insert(path, raw);
    }
    let index = event_index::persist("large-native", &references, &mut |page| {
        let sequence = files.len() + 1;
        let path = format!("receipts/{sequence:04}-event_reference_index.json");
        let raw = serde_json::to_vec(
            &json!({"schema":"cad-agent.nx-native-event/v2","job_id":"large-native",
            "sequence":sequence,"phase":"event_reference_index","observation":page}),
        )?;
        let reference = ArtifactReference {
            path: path.clone(),
            sha256: p::Digest::of(&raw).as_str().into(),
        };
        files.insert(path, raw);
        Ok(reference)
    })
    .unwrap();
    let mut readback = json!({"action":"inspect_assembly","assembly":"x".repeat(3*1024*1024),
        "partial_observations_form":"complete_event_reference_index","event_reference_index":index});
    event_index::persist_fields("large-native", &mut readback, &mut |c| {
        write_chunk(&mut files, c)
    })
    .unwrap();
    let restored = event_index::receive_readback(
        "large-native",
        event_index::READBACK_SCHEMA,
        &readback,
        &mut |p| Ok(files[p].clone()),
    )
    .unwrap();
    assert_eq!(restored["assembly"], json!("x".repeat(3 * 1024 * 1024)));
    assert_eq!(restored["partial_observations"], json!(references));
    assert_eq!(
        restored["load_status_records"],
        json!([references[1].clone()])
    );
    assert!(
        event_index::receive_readback(
            "large-native",
            "old-readback/1",
            &readback,
            &mut |_| panic!("wrong schema read")
        )
        .is_err()
    );
    let mut wrong_sequence = readback.clone();
    wrong_sequence["external_fields"][0]["chunks"][0]["path"] =
        json!("receipts/9999-readback_field_chunk.json");
    assert!(
        event_index::receive_readback(
            "large-native",
            event_index::READBACK_SCHEMA,
            &wrong_sequence,
            &mut |_| panic!("discontinuous field sequence read")
        )
        .is_err()
    );
    let path = "receipts/0002-native_load_status.json";
    files.get_mut(path).unwrap().push(b' ');
    assert!(
        event_index::receive_readback(
            "large-native",
            event_index::READBACK_SCHEMA,
            &readback,
            &mut |p| Ok(files[p].clone())
        )
        .is_err()
    );
}
