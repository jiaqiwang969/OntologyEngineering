//! Diagnostic byte and registration observations, never a verification receipt.
use super::{declared_evidence, declared_source_path, packet_declaration, packet_review};
use anyhow::{Context, Result, ensure};
use oe_contracts::{PacketOriginalBytesAudit, PacketSourceAuditReport};
use oe_core::{FileIdentity, SkillRoot, fingerprint_file, json::canonical_digest, no_symlinks};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

const MAX_ORIGINALS: usize = 96;
const MAX_HASH_BYTES: u64 = 4 * 1024 * 1024 * 1024;
// fingerprint_file checks its limit after a bounded 64 KiB read. Account for
// that last block if a file grows concurrently, including the final recheck.
const HASH_READ_HEADROOM: u64 = 64 * 1024;

struct Observation {
    status: &'static str,
    reason: Option<&'static str>,
    identity: Option<FileIdentity>,
    metadata: Option<fs::Metadata>,
}
impl Observation {
    fn unavailable(reason: &'static str) -> Self {
        Self {
            status: "unavailable",
            reason: Some(reason),
            identity: None,
            metadata: None,
        }
    }
}
fn same_metadata(a: &fs::Metadata, b: &fs::Metadata) -> bool {
    let same = a.file_type() == b.file_type()
        && a.len() == b.len()
        && a.modified().ok().is_some()
        && a.modified().ok() == b.modified().ok();
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        same && a.dev() == b.dev() && a.ino() == b.ino()
    }
    #[cfg(not(unix))]
    {
        same
    }
}

struct Originals {
    observations: BTreeMap<PathBuf, Observation>,
    inspected: usize,
    charged_bytes: u64,
    reserved_bytes: u64,
}
impl Originals {
    fn new() -> Self {
        Self {
            observations: BTreeMap::new(),
            inspected: 0,
            charged_bytes: 0,
            reserved_bytes: 0,
        }
    }
    fn observe(&mut self, path: &Path) {
        if self.observations.contains_key(path) {
            return;
        }
        let observed = if self.inspected >= MAX_ORIGINALS {
            Observation::unavailable("original_path_budget_exhausted")
        } else {
            self.inspected += 1;
            self.read_original(path).unwrap_or_else(|_| {
                Observation::unavailable("original_unreadable_unsafe_or_changed")
            })
        };
        self.observations.insert(path.to_owned(), observed);
    }
    // Only filesystem operations occur here: declaration errors are rejected earlier.
    fn read_original(&mut self, path: &Path) -> Result<Observation> {
        no_symlinks(path)?;
        let before = match fs::symlink_metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Observation {
                    status: "missing",
                    reason: Some("original_not_found"),
                    identity: None,
                    metadata: None,
                });
            }
            outcome => outcome?,
        };
        if !before.is_file() {
            return Ok(Observation::unavailable("original_not_regular"));
        }
        let bytes = before.len();
        if bytes > crate::citations::MAX_SOURCE_BYTES {
            return Ok(Observation::unavailable(
                "original_file_size_budget_exhausted",
            ));
        }
        // Reserve the final read now; a successful first read cannot consume its recheck.
        let charge = bytes.saturating_add(HASH_READ_HEADROOM);
        if charge.saturating_mul(2)
            > MAX_HASH_BYTES
                .saturating_sub(self.charged_bytes)
                .saturating_sub(self.reserved_bytes)
        {
            return Ok(Observation::unavailable(
                "original_hash_bytes_budget_exhausted",
            ));
        }
        self.charged_bytes += charge;
        let identity = fingerprint_file(path, bytes)?;
        no_symlinks(path)?;
        ensure!(
            same_metadata(&before, &fs::symlink_metadata(path)?),
            "original_changed_during_observation"
        );
        self.reserved_bytes += charge;
        Ok(Observation {
            status: "verified",
            reason: None,
            identity: Some(identity),
            metadata: Some(before),
        })
    }
    fn recheck(&mut self) {
        for (path, observation) in &mut self.observations {
            let stable =
                if let Some(identity) = &observation.identity {
                    let charge = identity.bytes + HASH_READ_HEADROOM;
                    self.charged_bytes += charge;
                    self.reserved_bytes -= charge;
                    (|| -> Result<bool> {
                        no_symlinks(path)?;
                        let before = fs::symlink_metadata(path)?;
                        let expected_metadata = observation
                            .metadata
                            .as_ref()
                            .context("original_observation_missing")?;
                        if !same_metadata(expected_metadata, &before) {
                            return Ok(false);
                        }
                        let current = fingerprint_file(path, identity.bytes)?;
                        no_symlinks(path)?;
                        Ok(current == *identity
                            && same_metadata(&before, &fs::symlink_metadata(path)?))
                    })()
                    .unwrap_or(false)
                } else if observation.status == "missing" {
                    no_symlinks(path).is_ok()
                        && fs::symlink_metadata(path)
                            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
                } else {
                    continue;
                };
            if !stable {
                observation.status = "unavailable";
                observation.reason = Some("original_changed_during_audit");
            }
        }
    }
    fn report(&self, root: &SkillRoot, results: &[Value]) -> Result<PacketOriginalBytesAudit> {
        let mut candidates = Vec::new();
        for result in results {
            let path = declared_source_path(root, &result["source_pdf"])?;
            let observed = &self.observations[&path];
            let expected_bytes = result
                .get("source_registration_binding")
                .map(|binding| &binding["source"]["bytes"])
                .or_else(|| result.get("source_bytes"))
                .cloned()
                .unwrap_or(Value::Null);
            let mut status = observed.status;
            let mut reason = observed.reason;
            if status == "verified" {
                let identity = observed.identity.as_ref().expect("verified identity");
                if result["source_sha256"] != identity.sha256 {
                    status = "changed";
                    reason = Some("original_sha256_differs");
                } else {
                    // A length contradicting the same digest is a bad declaration, not
                    // evidence that the original changed. Keep the strict binding rule.
                    ensure!(
                        expected_bytes.is_null() || expected_bytes.as_u64() == Some(identity.bytes),
                        "source_registration_binding_candidate_mismatch"
                    );
                }
            }
            candidates.push(json!({
                "candidate_id":result["id"],"source_pdf":result["source_pdf"],"status":status,
                "expected_sha256":result["source_sha256"],"expected_bytes":expected_bytes,
                "observed_sha256":observed.identity.as_ref().map(|i| &i.sha256),
                "observed_bytes":observed.identity.as_ref().map(|i| i.bytes),"reason":reason
            }));
        }
        let status = if candidates.is_empty() {
            "no_candidates"
        } else if candidates
            .iter()
            .all(|row| row["status"] == candidates[0]["status"])
        {
            candidates[0]["status"].as_str().expect("candidate status")
        } else {
            "mixed"
        };
        let report = PacketOriginalBytesAudit {
            status: status.into(),
            candidates,
            limits: json!({"max_distinct_originals":MAX_ORIGINALS,"max_hash_bytes":MAX_HASH_BYTES,
                "inspected_originals":self.inspected,"distinct_originals":self.observations.len(),
                "charged_hash_bytes":self.charged_bytes,"hash_read_headroom_bytes":HASH_READ_HEADROOM,
                "charge_meaning":"Conservative read upper bound including failed attempts and rechecks; not measured physical I/O.",
                "recheck_included":true}),
        };
        ensure!(
            report.validate().is_ok(),
            "packet_original_bytes_audit_contract_invalid"
        );
        Ok(report)
    }
}

/// Audit one explicit packet without requiring surviving originals or current Project
/// context. Malformed declarations remain errors; unavailable originals remain evidence
/// gaps. This result is neither an input to feedback nor a substitute for verify.
pub fn audit_sources(root: &SkillRoot, packet: &Value) -> Result<Value> {
    let (snapshot, results) = packet_declaration(packet)?;
    declared_evidence(root, &json!(results), &snapshot)?;
    packet_review(packet, &snapshot, &results)?;
    let mut originals = Originals::new();
    for result in &results {
        originals.observe(&declared_source_path(root, &result["source_pdf"])?);
    }
    let registration = crate::registration_freshness::audit_declarations(root, &results)?;
    originals.recheck();
    let report = PacketSourceAuditReport {
        schema: "ontology-engineering.packet-source-audit/v1".into(), status: "audited".into(),
        packet_sha256: canonical_digest(packet)?, candidate_count: results.len() as u64,
        original_bytes: originals.report(root, &results)?,
        source_registration_freshness: serde_json::from_value(registration)?,
        full_verification: "not_run".into(),
        meaning: "Diagnostic observations of declared source bytes and explicit local-PDF registrations only. packet_sha256 identifies canonical packet JSON, not file bytes. verified means original byte identity only. Historical declarations, context digest, text spans, local locators and binding relationships are checked, but original native pages, relocation/source-pack provenance and current Project context are not verified. No feedback write, engineering applicability, adoption or execution authority; strict verify-packet and feedback requirements remain unchanged.".into(),
    };
    ensure!(
        report.validate().is_ok(),
        "packet_source_audit_contract_invalid"
    );
    Ok(serde_json::to_value(report)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packet_original_recheck_rejects_changed_deleted_and_newly_present_files() {
        let temp = tempfile::tempdir().unwrap();
        let directory = fs::canonicalize(temp.path()).unwrap();
        let changed = directory.join("changed.pdf");
        let removed = directory.join("removed.pdf");
        let missing = directory.join("missing.pdf");
        fs::write(&changed, b"old bytes").unwrap();
        fs::write(&removed, b"old bytes").unwrap();
        let mut originals = Originals::new();
        for path in [&changed, &removed, &missing] {
            originals.observe(path);
        }
        assert_eq!(originals.observations[&changed].status, "verified");
        assert_eq!(originals.observations[&removed].status, "verified");
        assert_eq!(originals.observations[&missing].status, "missing");
        fs::write(&changed, b"new bytes").unwrap();
        fs::remove_file(&removed).unwrap();
        fs::write(&missing, b"new bytes").unwrap();
        originals.recheck();
        assert_eq!(originals.charged_bytes, 4 * (9 + HASH_READ_HEADROOM));
        assert_eq!(originals.reserved_bytes, 0);
        for path in [&changed, &removed, &missing] {
            assert_eq!(originals.observations[path].status, "unavailable");
            assert_eq!(
                originals.observations[path].reason,
                Some("original_changed_during_audit")
            );
        }
    }

    #[test]
    fn packet_original_hash_budget_reserves_recheck_without_reading_large_sparse_file() {
        let temp = tempfile::tempdir().unwrap();
        let directory = fs::canonicalize(temp.path()).unwrap();
        let path = directory.join("large.pdf");
        let file = fs::File::create(&path).unwrap();
        // The raw length would fit two reads; growth headroom must still prevent
        // even the first read from exhausting the final-recheck reservation.
        file.set_len(MAX_HASH_BYTES / 2).unwrap();
        let mut originals = Originals::new();
        originals.observe(&path);
        originals.recheck();
        assert_eq!(originals.observations[&path].status, "unavailable");
        assert_eq!(
            originals.observations[&path].reason,
            Some("original_hash_bytes_budget_exhausted")
        );
        assert_eq!(originals.charged_bytes, 0);
        assert_eq!(originals.reserved_bytes, 0);
    }
}
