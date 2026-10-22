//! Integrity verification utilities (port of `semantica/provenance/integrity.py`).
//!
//! SHA-256 checksums over the critical provenance fields detect tampering with stored entries;
//! `previous_checksum` chains every entry to its predecessor so that deleting a row is
//! detectable too (see `ProvenanceManager::verify_chain`).
//!
//! `compute_checksum`/`verify_checksum` accept either a [`ProvenanceEntry`] or a provenance
//! dict (`serde_json::Map`), like the original's `entry: Any`; the two paths format values
//! exactly as the original's attribute and `dict.get` branches do.

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use sem_utils::pycompat::fmt::{float_repr, py_repr, py_str, truthy};

use crate::schemas::ProvenanceEntry;

/// Something `compute_checksum` can hash: a [`ProvenanceEntry`] or a provenance dict.
pub trait ChecksumSource {
    /// The concatenated field string the SHA-256 checksum is computed over.
    fn checksum_payload(&self) -> String;
    /// The checksum stored on the entry (`entry.checksum` / `entry.get("checksum")`).
    fn stored_checksum(&self) -> Option<String>;
}

impl ChecksumSource for ProvenanceEntry {
    fn checksum_payload(&self) -> String {
        let mut data = String::new();
        data.push_str(&self.entity_type);
        data.push_str(&self.activity_id);
        data.push_str(&self.agent_id);
        data.push_str(&self.agent_type);
        data.push_str(&self.source_document);
        data.push_str(&self.timestamp);
        data.push_str(&float_repr(self.confidence));
        data.push_str(self.parent_entity_id.as_deref().unwrap_or(""));
        data.push_str(self.previous_version_id.as_deref().unwrap_or(""));
        data.push_str(self.derived_from_id.as_deref().unwrap_or(""));
        data.push_str(&self.used_entities.join(","));
        data.push_str(self.previous_checksum.as_deref().unwrap_or(""));
        data.push_str(if self.invalidated { "True" } else { "False" });
        data.push_str(self.invalidated_at_time.as_deref().unwrap_or(""));
        data.push_str(self.invalidated_by.as_deref().unwrap_or(""));
        data.push_str(self.invalidation_reason.as_deref().unwrap_or(""));
        data
    }

    fn stored_checksum(&self) -> Option<String> {
        self.checksum.clone()
    }
}

/// `f"{entry.get(key) or ''}"`.
fn or_empty(entry: &Map<String, Value>, key: &str) -> String {
    match entry.get(key) {
        Some(value) if truthy(value) => py_str(value),
        _ => String::new(),
    }
}

/// `','.join(entry.get("used_entities") or [])`: strings are joined as iterables of
/// characters and dicts as iterables of keys, as Python iterates them.
fn joined(entry: &Map<String, Value>, key: &str) -> String {
    match entry.get(key) {
        Some(value) if truthy(value) => match value {
            Value::Array(items) => items
                .iter()
                .map(|item| match item {
                    Value::String(s) => s.clone(),
                    other => py_repr(other),
                })
                .collect::<Vec<_>>()
                .join(","),
            Value::String(s) => s.chars().map(String::from).collect::<Vec<_>>().join(","),
            Value::Object(map) => map.keys().cloned().collect::<Vec<_>>().join(","),
            other => py_str(other),
        },
        _ => String::new(),
    }
}

impl ChecksumSource for Map<String, Value> {
    fn checksum_payload(&self) -> String {
        let confidence = match self.get("confidence") {
            Some(Value::Null) | None => "1.0".to_string(),
            Some(value) => py_str(value),
        };
        let invalidated = self.get("invalidated").map(truthy).unwrap_or(false);
        let mut data = String::new();
        for key in [
            "entity_type",
            "activity_id",
            "agent_id",
            "agent_type",
            "source_document",
            "timestamp",
        ] {
            data.push_str(&or_empty(self, key));
        }
        data.push_str(&confidence);
        for key in ["parent_entity_id", "previous_version_id", "derived_from_id"] {
            data.push_str(&or_empty(self, key));
        }
        data.push_str(&joined(self, "used_entities"));
        data.push_str(&or_empty(self, "previous_checksum"));
        data.push_str(if invalidated { "True" } else { "False" });
        for key in [
            "invalidated_at_time",
            "invalidated_by",
            "invalidation_reason",
        ] {
            data.push_str(&or_empty(self, key));
        }
        data
    }

    fn stored_checksum(&self) -> Option<String> {
        match self.get("checksum") {
            None | Some(Value::Null) => None,
            Some(Value::String(s)) => Some(s.clone()),
            Some(other) => Some(py_str(other)),
        }
    }
}

fn sha256_hex(data: &str) -> String {
    hex::encode(Sha256::digest(data.as_bytes()))
}

/// Compute the SHA-256 checksum of a provenance entry (or provenance dict).
///
/// Covers `entity_type`, `activity_id`, `agent_id`, `agent_type`, `source_document`,
/// `timestamp`, `confidence`, the lineage links (`parent_entity_id`, `previous_version_id`,
/// `derived_from_id`, `used_entities`), `previous_checksum` and the invalidation fields.
/// `entity_id` is deliberately excluded so that archiving a version under a new key keeps its
/// checksum (and every later entry chained from it) valid.
pub fn compute_checksum<E: ChecksumSource + ?Sized>(entry: &E) -> String {
    sha256_hex(&entry.checksum_payload())
}

/// Verify an entry against `expected_checksum`, or against its own stored checksum when
/// `None`. An entry without any checksum never verifies.
pub fn verify_checksum<E: ChecksumSource + ?Sized>(
    entry: &E,
    expected_checksum: Option<&str>,
) -> bool {
    let expected = match expected_checksum {
        Some(value) => value.to_string(),
        None => match entry.stored_checksum() {
            Some(value) => value,
            None => return false,
        },
    };
    compute_checksum(entry) == expected
}

/// SHA-256 checksum (hex) of arbitrary string data.
pub fn compute_data_checksum(data: &str) -> String {
    sha256_hex(data)
}

/// Verify `data` against `expected_checksum`.
pub fn verify_data_checksum(data: &str, expected_checksum: &str) -> bool {
    compute_data_checksum(data) == expected_checksum
}

/// SHA-256 checksum of a dict: the hash of `str(sorted(data.items()))`, so key order does not
/// matter.
pub fn compute_dict_checksum(data: &Map<String, Value>) -> String {
    sha256_hex(&sorted_items_repr(data))
}

/// Verify a dict against `expected_checksum`.
pub fn verify_dict_checksum(data: &Map<String, Value>, expected_checksum: &str) -> bool {
    compute_dict_checksum(data) == expected_checksum
}

/// `str(sorted(data.items()))`: a list of `(key, value)` tuple reprs ordered by key.
pub(crate) fn sorted_items_repr(data: &Map<String, Value>) -> String {
    let mut items: Vec<(&String, &Value)> = data.iter().collect();
    // Python orders str keys by code point, which is UTF-8 byte order.
    items.sort_by(|a, b| a.0.cmp(b.0));
    let parts: Vec<String> = items
        .iter()
        .map(|(key, value)| {
            format!(
                "({}, {})",
                py_repr(&Value::String((*key).clone())),
                py_repr(value)
            )
        })
        .collect();
    format!("[{}]", parts.join(", "))
}
