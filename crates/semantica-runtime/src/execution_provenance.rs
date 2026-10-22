//! Complete storage-free execution PROV Entry/Bundle and receipt content.
//!
//! The original checksum is deliberately narrower than the bundle hash. Pure
//! builders authenticate content, never execution, source approval or release.
//! The current BookRunner producer is a separate, same-instance entry point.
use anyhow::{Context, Result, ensure};
use semantica_contracts::{
    SemanticExecutionProvenanceBundle as Bundle, SemanticExecutionProvenanceEntry as Entry,
    SemanticExecutionProvenanceReceipt as Receipt, SemanticExecutionProvenanceReport as Report,
};
use serde_json::{Map, Value, json};

pub const CURRENT_PROFILE: &str = "semantica.execution-provenance-rust-observation/1";
const MAX_RECORDS: usize = 100_000;

pub fn canonical_json(value: &Value) -> Result<Vec<u8>> {
    let bytes = semantica_contracts::json::canonical_json(value)?;
    ensure!(
        bytes.len() <= crate::MAX_BYTES,
        "execution_provenance_byte_budget"
    );
    Ok(bytes)
}
fn hash(value: &Value) -> Result<String> {
    Ok(crate::sha256(canonical_json(value)?))
}
fn value<T: serde::Serialize>(input: &T) -> Result<Value> {
    Ok(serde_json::to_value(input)?)
}
fn object(input: &Value) -> Result<&Map<String, Value>> {
    input
        .as_object()
        .context("execution_provenance_object_required")
}
fn digest(input: &Value) -> bool {
    input.as_str().is_some_and(|s| {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    })
}
fn truth(input: &Value) -> bool {
    match input {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::String(v) => !v.is_empty(),
        Value::Array(v) => !v.is_empty(),
        Value::Object(v) => !v.is_empty(),
        Value::Number(v) => v.as_f64().map_or_else(
            || {
                v.to_string()
                    .trim_start_matches('-')
                    .bytes()
                    .any(|b| b != b'0')
            },
            |v| v != 0.0,
        ),
    }
}
fn text(input: &Value) -> Result<String> {
    crate::book_runner::python_value_string(input)
}
fn or_empty(input: &Value) -> Result<String> {
    if truth(input) {
        text(input)
    } else {
        Ok(String::new())
    }
}
fn clock(naive: bool) -> String {
    let now: chrono::DateTime<chrono::Utc> = std::time::SystemTime::now().into();
    let mut stamp = now.format("%Y-%m-%dT%H:%M:%S").to_string();
    let micros = now.timestamp_subsec_micros();
    if micros != 0 {
        stamp.push_str(&format!(".{micros:06}"));
    }
    if !naive {
        stamp.push('Z');
    }
    stamp
}

/// Original ProvenanceEntry.from_dict defaults in the JSON-compatible domain.
/// Python annotations are not validators: JSON values are retained verbatim.
/// The implicit original UTC-naive clock is resolved only at this boundary.
pub fn entry_from_dict(input: &Value, default_timestamp: Option<&str>) -> Result<Entry> {
    let supplied = object(input)?;
    for key in ["entity_id", "entity_type", "activity_id"] {
        ensure!(
            supplied.contains_key(key),
            "provenance_entry_required_field"
        );
    }
    let mut defaults = json!({
        "entity_id":null,"entity_type":null,"activity_id":null,
        "agent_id":"semantica","agent_type":"software_agent","is_automated":true,"role":null,
        "source_document":"","source_location":null,"source_quote":null,
        "timestamp":default_timestamp.map(str::to_owned).unwrap_or_else(||clock(true)),
        "first_seen":null,"last_updated":null,"confidence":1.0,"checksum":null,
        "sequence_id":null,"previous_checksum":null,"parent_entity_id":null,"used_entities":[],
        "previous_version_id":null,"derived_from_id":null,"activity_started_at_time":null,
        "activity_ended_at_time":null,"acted_on_behalf_of":null,"informed_by_activities":[],
        "valid_from":null,"valid_until":null,"revision_type":null,"supersedes":null,"bundle_id":null,
        "invalidated":false,"invalidated_at_time":null,"invalidated_by":null,"invalidation_reason":null,
        "start_index":null,"end_index":null,"credibility":null,"metadata":{},"version":"1.0"
    });
    let target = defaults.as_object_mut().expect("object literal");
    for (key, val) in supplied {
        ensure!(
            target.contains_key(key),
            "provenance_entry_unexpected_field"
        );
        target.insert(key.clone(), val.clone());
    }
    canonical_json(&defaults)?;
    Ok(serde_json::from_value(defaults)?)
}

fn checksum_content(entry: &Value, object_entry: bool) -> Result<String> {
    object(entry)?;
    let mut content = String::new();
    for name in [
        "entity_type",
        "activity_id",
        "agent_id",
        "agent_type",
        "source_document",
        "timestamp",
    ] {
        let raw = &entry[name];
        let direct = object_entry
            && ["entity_type", "activity_id", "source_document", "timestamp"].contains(&name);
        content.push_str(&if direct { text(raw)? } else { or_empty(raw)? });
    }
    let confidence = &entry["confidence"];
    content.push_str(&if confidence.is_null() && !object_entry {
        "1.0".into()
    } else {
        text(confidence)?
    });
    for name in ["parent_entity_id", "previous_version_id", "derived_from_id"] {
        content.push_str(&or_empty(&entry[name])?);
    }
    let used = &entry["used_entities"];
    if truth(used) {
        // Python join also accepts a string iterable, joining Unicode codepoints.
        if let Some(s) = used.as_str() {
            content.push_str(
                &s.chars()
                    .map(|c| c.to_string())
                    .collect::<Vec<_>>()
                    .join(","),
            );
        } else if let Some(values) = used.as_object() {
            content.push_str(
                &values
                    .keys()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .join(","),
            );
        } else {
            let values = used
                .as_array()
                .context("provenance_used_entities_not_iterable")?;
            let strings = values
                .iter()
                .map(|v| v.as_str().context("provenance_used_entity_not_string"))
                .collect::<Result<Vec<_>>>()?;
            content.push_str(&strings.join(","));
        }
    }
    content.push_str(&or_empty(&entry["previous_checksum"])?);
    content.push_str(if truth(&entry["invalidated"]) {
        "True"
    } else {
        "False"
    });
    for name in [
        "invalidated_at_time",
        "invalidated_by",
        "invalidation_reason",
    ] {
        content.push_str(&or_empty(&entry[name])?);
    }
    Ok(crate::sha256(content))
}
/// Original integrity.compute_checksum(dict) with Python truthiness and strings.
pub fn compute_checksum(entry: &Value) -> Result<String> {
    checksum_content(entry, false)
}
/// Original integrity.compute_checksum(ProvenanceEntry), including its distinct
/// null-confidence/object semantics. This distinction is not silently collapsed.
pub fn compute_entry_checksum(entry: &Entry) -> Result<String> {
    checksum_content(&value(entry)?, true)
}
pub fn verify_checksum(entry: &Value, expected: Option<&str>) -> Result<bool> {
    let expected = expected
        .map(json_string)
        .unwrap_or_else(|| entry["checksum"].clone());
    if expected.is_null() {
        return Ok(false);
    }
    Ok(expected == json!(compute_checksum(entry)?))
}
fn json_string(s: &str) -> Value {
    json!(s)
}
pub fn compute_data_checksum(data: &str) -> String {
    crate::sha256(data)
}
pub fn verify_data_checksum(data: &str, expected: &str) -> bool {
    compute_data_checksum(data) == expected
}
pub fn compute_dict_checksum(data: &Value) -> Result<String> {
    let mut items: Vec<_> = object(data)?.iter().collect();
    items.sort_by(|a, b| a.0.cmp(b.0));
    let mut pairs = Vec::new();
    for (key, val) in items {
        pairs.push(format!(
            "({}, {})",
            crate::book_runner::python_value_repr(&json!(key))?,
            crate::book_runner::python_value_repr(val)?
        ));
    }
    Ok(crate::sha256(format!("[{}]", pairs.join(", "))))
}
pub fn verify_dict_checksum(data: &Value, expected: &str) -> Result<bool> {
    Ok(compute_dict_checksum(data)? == expected)
}

/// Canonical record DTO construction does not authenticate an execution actor.
pub fn validate_record(entry_json: &str, checksum: &str) -> Result<Value> {
    let parsed = semantica_contracts::json::strict_json(entry_json.as_bytes())?;
    ensure!(
        canonical_json(&parsed)? == entry_json.as_bytes(),
        "provenance_entry_json_not_canonical"
    );
    ensure!(digest(&json!(checksum)), "provenance_checksum_not_sha256");
    ensure!(
        parsed["checksum"] == checksum,
        "provenance_record_checksum_binding"
    );
    Ok(parsed)
}
fn record(entry: Value) -> Result<Entry> {
    let checksum = compute_checksum(&entry)?;
    let mut entry = entry;
    entry["checksum"] = json!(checksum);
    let raw = String::from_utf8(canonical_json(&entry)?)?;
    validate_record(&raw, &checksum)?;
    Ok(serde_json::from_value(entry)?)
}
fn make_entry(
    entity_id: String,
    entity_type: &str,
    activity_id: &str,
    agent: &str,
    source: &str,
    time: &Value,
    bundle: &str,
    metadata: Value,
    parent: Option<&str>,
    derived: Option<&str>,
    used: Vec<String>,
) -> Result<Entry> {
    entry_from_dict(
        &json!({"entity_id":entity_id,"entity_type":entity_type,"activity_id":activity_id,"agent_id":agent,"agent_type":"software_agent","is_automated":true,"role":"generator","source_document":source,"timestamp":time,"first_seen":time,"last_updated":time,"confidence":1.0,"parent_entity_id":parent,"derived_from_id":derived,"used_entities":used,"activity_started_at_time":time,"activity_ended_at_time":time,"bundle_id":bundle,"metadata":metadata}),
        Some("unused-explicit-entry-time"),
    )
}

/// Original deterministic builder. Bindings are pure JSON observations and
/// report statuses remain declarations until an actual producer supplies them.
pub fn build_provenance_bundle(bindings: &Value, generated_at: Option<&str>) -> Result<Bundle> {
    let when = generated_at.map(json_string);
    build_provenance_bundle_json(bindings, when.as_ref())
}

/// Original JSON-compatible time domain: a truthy value is retained verbatim;
/// null/false/empty values choose the clock at the boundary, as in Python.
pub fn build_provenance_bundle_json(
    bindings: &Value,
    generated_at: Option<&Value>,
) -> Result<Bundle> {
    object(bindings)?;
    let time = generated_at
        .filter(|v| truth(v))
        .cloned()
        .unwrap_or_else(|| json!(clock(false)));
    let binding_digest = hash(bindings)?;
    let bundle_id = format!(
        "urn:semantica:provenance-bundle:{}",
        hash(&json!({"bindings":bindings,"time":time}))?
    );
    let field = |key: &str| -> Result<String> {
        bindings
            .get(key)
            .map(text)
            .unwrap_or_else(|| Ok(String::new()))
    };
    let version = field("runtime_version")?;
    let package = field("package_id")?;
    let package_version = field("package_version")?;
    let package_digest = field("package_digest")?;
    let dataset_digest = field("dataset_sha256")?;
    let source = format!("{package}@{package_version}");
    let agent = format!(
        "semantica:{}",
        if version.is_empty() {
            "unknown"
        } else {
            &version
        }
    );
    let package_entity = format!(
        "urn:semantica:package:{}",
        if package_digest.is_empty() {
            "missing"
        } else {
            &package_digest
        }
    );
    let dataset_entity = format!(
        "urn:semantica:dataset:{}",
        if dataset_digest.is_empty() {
            "missing"
        } else {
            &dataset_digest
        }
    );
    let mut entries = vec![make_entry(
        package_entity.clone(),
        "semantic_package",
        "semantic.package.load",
        &agent,
        &source,
        &time,
        &bundle_id,
        json!({"package_digest":package_digest}),
        None,
        None,
        vec![],
    )?];
    let mut assets = Vec::new();
    if let Some(hashes) = bindings["asset_hashes"].as_object() {
        let mut sorted: Vec<_> = hashes.iter().collect();
        sorted.sort_by(|a, b| a.0.cmp(b.0));
        for (id, digest) in sorted {
            ensure!(entries.len() < MAX_RECORDS, "provenance_record_budget");
            let entity = format!(
                "urn:semantica:asset:{}",
                crate::sha256(format!("{package_digest}:{id}:{}", text(digest)?))
            );
            assets.push(entity.clone());
            entries.push(make_entry(
                entity,
                "semantic_package_asset",
                "semantic.package.load",
                &agent,
                &source,
                &time,
                &bundle_id,
                json!({"asset_id":id,"sha256":digest}),
                Some(&package_entity),
                Some(&package_entity),
                vec![],
            )?);
        }
    }
    entries.push(make_entry(
        dataset_entity.clone(),
        "rdf_dataset_snapshot",
        "semantic.package.execute",
        &agent,
        &source,
        &time,
        &bundle_id,
        json!({"dataset_sha256":dataset_digest}),
        Some(&package_entity),
        None,
        assets,
    )?);
    let mut reports = Vec::new();
    if let Some(hashes) = bindings["report_hashes"].as_object() {
        let mut sorted: Vec<_> = hashes.iter().collect();
        sorted.sort_by(|a, b| a.0.cmp(b.0));
        for (kind, digest) in sorted {
            ensure!(entries.len() < MAX_RECORDS, "provenance_record_budget");
            let entity = format!("urn:semantica:execution-report:{}", text(digest)?);
            reports.push(entity.clone());
            entries.push(make_entry(
                entity,
                "semantic_execution_report",
                "semantic.package.verify",
                &agent,
                &source,
                &time,
                &bundle_id,
                json!({"report_kind":kind,"sha256":digest}),
                Some(&dataset_entity),
                Some(&dataset_entity),
                vec![],
            )?);
        }
    }
    let mut used = vec![package_entity, dataset_entity.clone()];
    used.extend(reports);
    entries.push(make_entry(
        format!("urn:semantica:execution:{binding_digest}"),
        "semantic_package_execution",
        "semantic.release.evidence",
        &agent,
        &source,
        &time,
        &bundle_id,
        json!({"bindings_sha256":binding_digest}),
        Some(&dataset_entity),
        None,
        used,
    )?);
    ensure!(entries.len() <= MAX_RECORDS, "provenance_record_budget");
    let mut previous = Value::Null;
    let mut records = Vec::new();
    for (i, entry) in entries.into_iter().enumerate() {
        let mut entry = value(&entry)?;
        entry["sequence_id"] = json!(i + 1);
        entry["previous_checksum"] = previous;
        let entry = record(entry)?;
        previous = entry.checksum.clone();
        records.push(entry);
    }
    let mut bundle = Bundle {
        schema_version: "1.0".into(),
        bundle_id,
        generated_at: time,
        bindings: bindings.clone(),
        records,
        bundle_sha256: "0".repeat(64),
    };
    bundle.bundle_sha256 = hash(&without(&value(&bundle)?, "bundle_sha256")?)?;
    ensure!(
        verify_bundle(&value(&bundle)?)?,
        "provenance_built_bundle_invalid"
    );
    Ok(bundle)
}
fn without(input: &Value, key: &str) -> Result<Value> {
    let mut content = object(input)?.clone();
    content.remove(key);
    Ok(Value::Object(content))
}
fn sequence_equals(input: &Value, expected: usize) -> bool {
    match input {
        Value::Bool(b) => usize::from(*b) == expected,
        Value::Number(n) => {
            n.as_u64() == Some(expected as u64) || n.as_f64() == Some(expected as f64)
        }
        _ => false,
    }
}
/// Original record checks, exact insertion chain and full content hash. Sequence
/// equality preserves Python numeric equality (including True == 1).
pub fn verify_bundle(bundle: &Value) -> Result<bool> {
    object(bundle)?;
    for key in [
        "schema_version",
        "bundle_id",
        "generated_at",
        "bindings",
        "records",
        "bundle_sha256",
    ] {
        ensure!(
            bundle.get(key).is_some(),
            "provenance_bundle_required_field"
        );
    }
    ensure!(
        bundle["bundle_id"]
            .as_str()
            .is_some_and(|s| !semantica_contracts::json::python_strip(s).is_empty()),
        "provenance_bundle_id_empty"
    );
    ensure!(
        digest(&bundle["bundle_sha256"]),
        "provenance_bundle_digest_invalid"
    );
    let records = bundle["records"]
        .as_array()
        .context("provenance_records_required")?;
    ensure!(!records.is_empty(), "provenance_bundle_records_empty");
    ensure!(records.len() <= MAX_RECORDS, "provenance_record_budget");
    let mut previous = Value::Null;
    for (i, record) in records.iter().enumerate() {
        object(record)?;
        ensure!(
            digest(&record["checksum"]),
            "provenance_record_digest_invalid"
        );
        if !verify_checksum(record, None)?
            || !sequence_equals(&record["sequence_id"], i + 1)
            || record["previous_checksum"] != previous
        {
            return Ok(false);
        }
        previous = record["checksum"].clone();
    }
    let content = json!({"schema_version":bundle["schema_version"],"bundle_id":bundle["bundle_id"],"generated_at":bundle["generated_at"],"bindings":bundle["bindings"],"records":records});
    Ok(bundle["bundle_sha256"] == hash(&content)?)
}

pub fn execution_report(kind: &str, payload: &Value, status: &str) -> Result<Report> {
    ensure!(
        !semantica_contracts::json::python_strip(kind).is_empty(),
        "execution_report_kind_empty"
    );
    ensure!(
        ["passed", "failed", "blocked"].contains(&status),
        "execution_report_status_invalid"
    );
    let sha256 = hash(&json!({"kind":kind,"status":status,"payload":payload}))?;
    Ok(Report {
        kind: kind.into(),
        status: status.into(),
        payload: payload.clone(),
        sha256,
    })
}
pub fn missing_execution_report(kind: &str) -> Result<Report> {
    execution_report(
        kind,
        &json!({"reason":"required execution report was not supplied"}),
        "blocked",
    )
}
pub fn verify_report(report: &Value) -> Result<bool> {
    object(report)?;
    ensure!(
        report["kind"]
            .as_str()
            .is_some_and(|s| !semantica_contracts::json::python_strip(s).is_empty()),
        "execution_report_kind_empty"
    );
    ensure!(
        report["status"]
            .as_str()
            .is_some_and(|s| ["passed", "failed", "blocked"].contains(&s)),
        "execution_report_status_invalid"
    );
    ensure!(digest(&report["sha256"]), "execution_report_digest_invalid");
    ensure!(
        report.get("payload").is_some(),
        "execution_report_payload_missing"
    );
    Ok(report["sha256"]
        == hash(
            &json!({"kind":report["kind"],"status":report["status"],"payload":report["payload"]}),
        )?)
}
pub fn receipt_bindings(receipt: &Value) -> Result<Value> {
    object(receipt)?;
    let mut reports = Map::new();
    for field in [
        "capability_report",
        "cq_report",
        "shacl_report",
        "oracle_report",
    ] {
        let report = &receipt[field];
        reports.insert(
            report["kind"]
                .as_str()
                .context("execution_report_kind_not_string")?
                .into(),
            report["sha256"].clone(),
        );
    }
    let mut map = Map::new();
    for field in [
        "runtime_version",
        "runtime_commit",
        "runtime_artifact_sha256",
        "package_id",
        "package_version",
        "package_digest",
        "asset_hashes",
        "chapter_contract_sha256",
        "dataset_sha256",
        "output_hashes",
    ] {
        map.insert(field.into(), receipt[field].clone());
    }
    map.insert("report_hashes".into(), Value::Object(reports));
    Ok(Value::Object(map))
}
/// Original data-only receipt builder. It does not accept an execution token,
/// verify current source, authorize release, or infer that report data ran.
pub fn build_execution_receipt(input: &Value) -> Result<Receipt> {
    object(input)?;
    let field = |key: &str| -> Result<String> {
        input
            .get(key)
            .map(text)
            .context("execution_receipt_field_missing")?
    };
    let mut receipt = Receipt {
        schema_version: "1.0".into(),
        created_at: input
            .get("created_at")
            .context("execution_receipt_created_at_missing")?
            .clone(),
        runtime_version: field("runtime_version")?,
        runtime_commit: field("runtime_commit")?,
        runtime_artifact_sha256: field("runtime_artifact_sha256")?,
        package_id: field("package_id")?,
        package_version: field("package_version")?,
        package_digest: field("package_digest")?,
        chapter_contract_sha256: field("chapter_contract_sha256")?,
        dataset_sha256: field("dataset_sha256")?,
        asset_hashes: input["asset_hashes"].clone(),
        dataset_quad_count: input["dataset_quad_count"].clone(),
        dataset_revision: input["dataset_revision"].clone(),
        output_hashes: input
            .get("output_hashes")
            .filter(|v| truth(v))
            .cloned()
            .unwrap_or_else(|| json!({})),
        capability_report: serde_json::from_value(input["capability_report"].clone())?,
        cq_report: serde_json::from_value(input["cq_report"].clone())?,
        shacl_report: serde_json::from_value(input["shacl_report"].clone())?,
        oracle_report: serde_json::from_value(input["oracle_report"].clone())?,
        provenance_bundle: build_provenance_bundle(&json!({}), Some("provisional"))?,
        receipt_sha256: "0".repeat(64),
    };
    for field in [&receipt.asset_hashes, &receipt.output_hashes] {
        object(field)?;
    }
    for field in [&mut receipt.asset_hashes, &mut receipt.output_hashes] {
        let map = object(field)?
            .iter()
            .map(|(k, v)| Ok((k.clone(), json!(text(v)?))))
            .collect::<Result<Map<_, _>>>()?;
        *field = Value::Object(map);
    }
    for report in [
        &receipt.capability_report,
        &receipt.cq_report,
        &receipt.shacl_report,
        &receipt.oracle_report,
    ] {
        // Constructor validity is required; the original builder can still
        // retain a well-formed report whose declared digest does not match.
        let _ = verify_report(&value(report)?)?;
    }
    let bindings = receipt_bindings(&value(&receipt)?)?;
    receipt.provenance_bundle = build_provenance_bundle_json(&bindings, Some(&receipt.created_at))?;
    receipt.receipt_sha256 = hash(&without(&value(&receipt)?, "receipt_sha256")?)?;
    Ok(receipt)
}
pub fn verify_receipt(receipt: &Value) -> Result<bool> {
    let fields = object(receipt)?;
    for key in [
        "schema_version",
        "created_at",
        "runtime_version",
        "runtime_commit",
        "runtime_artifact_sha256",
        "package_id",
        "package_version",
        "package_digest",
        "asset_hashes",
        "chapter_contract_sha256",
        "dataset_sha256",
        "dataset_quad_count",
        "dataset_revision",
        "capability_report",
        "cq_report",
        "shacl_report",
        "oracle_report",
        "output_hashes",
        "provenance_bundle",
        "receipt_sha256",
    ] {
        ensure!(fields.contains_key(key), "execution_receipt_required_field");
    }
    ensure!(
        receipt["schema_version"] == "1.0",
        "execution_receipt_schema_unsupported"
    );
    ensure!(
        digest(&receipt["receipt_sha256"]),
        "execution_receipt_digest_invalid"
    );
    object(&receipt["asset_hashes"])?;
    object(&receipt["output_hashes"])?;
    for field in [
        "capability_report",
        "cq_report",
        "shacl_report",
        "oracle_report",
    ] {
        if !verify_report(&receipt[field])? {
            return Ok(false);
        }
    }
    Ok(verify_bundle(&receipt["provenance_bundle"])?
        && receipt["receipt_sha256"] == hash(&without(receipt, "receipt_sha256")?)?)
}
/// Binding equality is separate from original receipt integrity: release must
/// require both, and content construction alone cannot grant its authority.
pub fn verify_receipt_bindings(receipt: &Value) -> Result<bool> {
    Ok(semantica_contracts::json::python_equal(
        &receipt["provenance_bundle"]["bindings"],
        &receipt_bindings(receipt)?,
    )?)
}
/// Strict wire entry: duplicate map keys, nonfinite values and excess nesting
/// are rejected before the original DTO constructor/integrity checks.
pub fn decode_receipt(raw: &[u8]) -> Result<Receipt> {
    ensure!(
        raw.len() <= crate::MAX_BYTES,
        "execution_provenance_byte_budget"
    );
    let parsed = semantica_contracts::json::strict_json(raw)?;
    let _ = verify_receipt(&parsed)?;
    Ok(serde_json::from_value(parsed)?)
}
