//! Pure original promotion registry content algorithms. The injected Store
//! owns managed byte reads, locking, CAS, atomic publication and recovery.
//! Contents and hashes cannot establish actual execution or release authority.
use crate::{refinery, refinery_contracts as wire};
use anyhow::{Context, Result, ensure};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

const DESCRIPTOR: &[&str] = &[
    "package_id",
    "version",
    "package_sha256",
    "manifest_sha256",
    "promotion_record_sha256",
    "delta_sha256",
    "engagement_receipt_sha256",
    "regression_evidence_sha256",
    "release_evidence_sha256",
    "subject_execution_suite_sha256",
    "transition_context_sha256",
    "transition_context_object_sha256",
    "provenance_closure_sha256",
    "provenance_closure_object_sha256",
    "authorization_sha256",
    "authorization_object_sha256",
    "book_impact",
    "promoted_at",
];
const EVENT: &[&str] = &[
    "schema_version",
    "sequence",
    "previous_event_sha256",
    "operation",
    "package_id",
    "version",
    "package_sha256",
    "promotion_record_sha256",
    "packages_sha256",
    "recorded_at",
    "event_sha256",
];
fn text<'a>(value: &'a Value, field: &str) -> Result<&'a str> {
    let text = value[field].as_str().context("registry_text_required")?;
    ensure!(
        !text.is_empty() && semantica_contracts::json::python_strip(text) == text,
        "registry_text_empty_or_surrounding_whitespace:{field}"
    );
    Ok(text)
}
fn sha<'a>(value: &'a Value, field: &str) -> Result<&'a str> {
    let text = text(value, field)?;
    ensure!(wire::valid_sha(text), "registry_sha256_invalid:{field}");
    Ok(text)
}
fn package_id(value: &Value) -> Result<&str> {
    wire::opaque_identifier(&value["package_id"])
}
fn sequence(value: &Value, minimum: u64) -> Result<u64> {
    let number = value["sequence"]
        .as_u64()
        .context("registry_sequence_invalid")?;
    ensure!(number >= minimum, "registry_sequence_invalid");
    Ok(number)
}
fn hash_without(value: &Value, field: &str) -> Result<String> {
    let mut content = value
        .as_object()
        .context("registry_object_required")?
        .clone();
    content.remove(field);
    refinery::hash(&Value::Object(content))
}

/// Original event schema accepts absent predecessor only for sequence one;
/// every other field is independently required by its original content check.
pub fn registry_event(value: &Value) -> Result<()> {
    let fields = value
        .as_object()
        .context("registry_event_mapping_required")?;
    ensure!(
        fields.keys().all(|key| EVENT.contains(&key.as_str())),
        "registry_event_unknown_field"
    );
    ensure!(
        text(value, "schema_version")? == "1.0",
        "registry_event_schema_invalid"
    );
    let index = sequence(value, 1)?;
    if index == 1 {
        ensure!(
            value["previous_event_sha256"].is_null(),
            "registry_first_event_has_predecessor"
        );
    } else {
        sha(value, "previous_event_sha256")?;
    }
    ensure!(
        value["operation"] == "promote",
        "registry_event_operation_invalid"
    );
    package_id(value)?;
    text(value, "version")?;
    for field in [
        "package_sha256",
        "promotion_record_sha256",
        "packages_sha256",
    ] {
        sha(value, field)?;
    }
    wire::timestamp(&value["recorded_at"])?;
    ensure!(
        sha(value, "event_sha256")? == hash_without(value, "event_sha256")?,
        "registry_event_hash_mismatch"
    );
    Ok(())
}

/// The original raw record read binds exact bytes-derived content and identity.
/// Descriptor parsing separately requires the complete promotion schema.
pub fn record_data(record: &Value, package: &str, version: &str, expected: &str) -> Result<()> {
    let actual = hash_without(record, "promotion_record_sha256")?;
    ensure!(
        record["promotion_record_sha256"] == actual && expected == actual,
        "registry_promotion_record_hash_mismatch"
    );
    ensure!(
        record["package_id"] == package && record["version"] == version,
        "registry_promotion_record_identity_mismatch"
    );
    Ok(())
}

fn runtime_source(value: &Value) -> Result<()> {
    if value["schema"] == "semantica.refinery-rust-source/1" {
        let source: semantica_contracts::SemanticRefineryRustSource =
            serde_json::from_value(value.clone())?;
        source.validate().map_err(|errors| {
            anyhow::anyhow!("registry_rust_source_invalid:{}", errors.join(";"))
        })?;
        ensure!(
            source.status == "source_bound" && source.executable_size_bytes > 0,
            "registry_rust_source_observation_incomplete"
        );
        // This is the explicitly declared current source observation domain.
        // Neither source_bound nor caller origin strings are authentication.
    } else {
        let source = wire::runtime_source(value)?;
        let commit = source["runtime_commit"].as_str().unwrap();
        ensure!(
            commit.len() == 40
                && commit
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                && source["runtime_artifact_sha256"]
                    .as_str()
                    .is_some_and(wire::valid_sha)
                && source["runtime_version"]
                    .as_str()
                    .is_some_and(|s| !s.is_empty()),
            "registry_legacy_runtime_source_incomplete"
        );
    }
    Ok(())
}

pub fn promotion_record(value: &Value) -> Result<()> {
    let fields = value
        .as_object()
        .context("registry_promotion_mapping_required")?;
    let additional = ["schema_version", "runtime_source", "status"];
    ensure!(fields.len() == DESCRIPTOR.len() + additional.len()
        && fields.keys().all(|key| DESCRIPTOR.contains(&key.as_str()) || additional.contains(&key.as_str())),
        "registry_promotion_schema_incomplete_or_unknown");
    ensure!(
        text(value, "schema_version")? == "1.0",
        "registry_promotion_schema_invalid"
    );
    ensure!(
        value["status"] == "promoted",
        "registry_promotion_status_invalid"
    );
    runtime_source(&value["runtime_source"])?;
    ensure!(
        value["promotion_record_sha256"] == hash_without(value, "promotion_record_sha256")?,
        "registry_promotion_record_digest_mismatch"
    );
    Ok(())
}

pub fn descriptor(record: &Value) -> Result<Value> {
    promotion_record(record)?;
    package_id(record)?;
    text(record, "version")?;
    for field in &DESCRIPTOR[2..16] {
        sha(record, field)?;
    }
    ensure!(
        ["none", "vol1-method", "vol2-iso-exemplar", "both"]
            .contains(&text(record, "book_impact")?),
        "registry_descriptor_book_impact_invalid"
    );
    wire::timestamp(&record["promoted_at"])?;
    Ok(Value::Object(
        DESCRIPTOR
            .iter()
            .map(|field| ((*field).into(), record[*field].clone()))
            .collect(),
    ))
}

fn document(registry_id: &str, pointer: &Value) -> Result<u64> {
    ensure!(
        pointer.is_object() && pointer["schema_version"] == "1.0",
        "registry_schema_invalid"
    );
    ensure!(
        pointer["registry_id"] == registry_id,
        "registry_identity_mismatch"
    );
    ensure!(
        pointer["packages"].is_object(),
        "registry_packages_mapping_required"
    );
    ensure!(
        pointer["packages_sha256"] == refinery::hash(&pointer["packages"])?,
        "registry_package_index_hash_mismatch"
    );
    let count = sequence(pointer, 0)?;
    if count == 0 {
        ensure!(
            pointer["last_event_sha256"].is_null(),
            "registry_empty_has_last_event"
        );
    } else {
        sha(pointer, "last_event_sha256")?;
    }
    Ok(count)
}

/// Reconstruct immutable events independently of the mutable pointer. The
/// application can use this same function for original one-event torn recovery.
pub fn reconstruct_events(
    events: &[(String, Value)],
    records: &BTreeMap<(String, String), Value>,
) -> Result<(Value, Value)> {
    let mut previous = Value::Null;
    let mut packages = Map::new();
    for (position, (name, event)) in events.iter().enumerate() {
        registry_event(event)?;
        let index = position + 1;
        ensure!(
            event["sequence"].as_u64() == Some(index as u64)
                && event["previous_event_sha256"] == previous,
            "registry_event_chain_broken"
        );
        let actual = hash_without(event, "event_sha256")?;
        ensure!(
            event["event_sha256"] == actual && name == &format!("{index:06}-{actual}.json"),
            "registry_event_filename_or_integrity_invalid"
        );
        let id = package_id(event)?;
        let version = text(event, "version")?;
        let expected = sha(event, "promotion_record_sha256")?;
        let record = records
            .get(&(id.into(), version.into()))
            .context("registry_promotion_record_missing")?;
        record_data(record, id, version, expected)?;
        ensure!(
            record["package_sha256"] == event["package_sha256"],
            "registry_event_package_record_digest_mismatch"
        );
        let package = packages.entry(id.to_owned()).or_insert_with(|| {
            json!({
                "storage_key":crate::sha256(id.as_bytes()), "current_version":null,
                "current_package_sha256":null, "versions":{}
            })
        });
        let versions = package["versions"]
            .as_object_mut()
            .context("registry_internal_versions_mapping_required")?;
        ensure!(
            !versions.contains_key(version),
            "registry_immutable_version_repeated"
        );
        versions.insert(
            version.into(),
            json!({"version_key":crate::sha256(version.as_bytes()), "record_sha256":expected}),
        );
        package["storage_key"] = json!(crate::sha256(id.as_bytes()));
        package["current_version"] = json!(version);
        package["current_package_sha256"] = record["package_sha256"].clone();
        ensure!(
            event["packages_sha256"] == refinery::hash(&Value::Object(packages.clone()))?,
            "registry_event_package_index_digest_invalid"
        );
        previous = json!(actual);
    }
    Ok((Value::Object(packages), previous))
}

pub fn reconstruct(
    registry_id: &str,
    pointer: &Value,
    events: &[(String, Value)],
    records: &BTreeMap<(String, String), Value>,
) -> Result<Value> {
    let count = document(registry_id, pointer)?;
    ensure!(
        count == events.len() as u64,
        "registry_event_count_mismatch"
    );
    let (packages, previous) = reconstruct_events(events, records)?;
    ensure!(
        pointer["last_event_sha256"] == previous,
        "registry_pointer_last_event_mismatch"
    );
    ensure!(
        semantica_contracts::json::canonical_json(&pointer["packages"])?
            == semantica_contracts::json::canonical_json(&packages)?,
        "registry_pointer_differs_from_immutable_packages"
    );
    Ok(packages)
}
