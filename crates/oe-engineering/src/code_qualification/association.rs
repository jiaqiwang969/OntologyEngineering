//! Derived associations for the existing qualification owner; never acceptance.
use anyhow::{Context, Result, ensure};
use oe_contracts::{
    ArtifactReference, CodeArtifactEligibilityRequest, CodeArtifactEligibilityResult,
    CodeScopeAssociationIndex, CodeScopeImplementationAssociation, CodeScopeItemEvidence,
    CodeScopeWitnessReport, CodeSourceArtifact,
};
use serde_json::Value;
use std::{collections::BTreeSet, path::Path};

const MAPPINGS: &str = "runtime/semantica-migration-mappings.json";
pub(super) const INDEX_BYTES: usize = 16 * 1024 * 1024;

/// Captured from the selected candidate, never the version-owner/driver root.
#[derive(Debug, PartialEq)]
pub(super) struct CandidateSnapshot {
    audit: Value,
    mappings_sha256: Option<String>,
    implementations: Vec<CodeScopeImplementationAssociation>,
}

impl CandidateSnapshot {
    pub(super) fn capture(candidate: &CodeSourceArtifact) -> Result<Self> {
        let root = Path::new(&candidate.source_root);
        let actual = oe_evolution::public_manifest_matching(root, &candidate.manifest)?;
        ensure!(
            actual == candidate.manifest
                && oe_evolution::digest(&actual)? == candidate.manifest_sha256,
            "scope_association_candidate_manifest_changed"
        );
        // The same frozen inventory gate, applied to the actual candidate.
        let audit = oe_evolution::scope_witness::inventory(root)?;
        let snapshot = Self::from_audit(audit, &actual)?;
        ensure!(
            oe_evolution::public_manifest_matching(root, &candidate.manifest)? == actual,
            "scope_association_candidate_changed_during_capture"
        );
        Ok(snapshot)
    }

    pub(super) fn recheck(&self, candidate: &CodeSourceArtifact) -> Result<()> {
        ensure!(
            Self::capture(candidate)? == *self,
            "scope_association_candidate_snapshot_changed"
        );
        Ok(())
    }

    fn from_audit(audit: Value, manifest: &Value) -> Result<Self> {
        let files = manifest["files"]
            .as_object()
            .context("scope_association_manifest_files")?;
        let file_sha = |path: &str| -> Result<Option<String>> {
            files
                .get(path)
                .map(|file| {
                    let sha = file["sha256"]
                        .as_str()
                        .context("scope_association_file_digest_missing")?;
                    ensure!(
                        sha.len() == 64
                            && sha
                                .bytes()
                                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)),
                        "scope_association_file_digest_invalid"
                    );
                    Ok(sha.to_owned())
                })
                .transpose()
        };
        let mappings_sha256 = file_sha(MAPPINGS)?;
        let mut implementations = Vec::new();
        for declaration in audit["declared_mappings"]
            .as_array()
            .context("scope_association_declarations_missing")?
        {
            let module = declaration["module"]
                .as_str()
                .context("scope_association_module_missing")?;
            let path = declaration["rust_path"]
                .as_str()
                .context("scope_association_path_missing")?;
            let digest = file_sha(path)?;
            for scope_id in declaration["scope_ids"]
                .as_array()
                .context("scope_association_declaration_ids_missing")?
            {
                implementations.push(CodeScopeImplementationAssociation {
                    scope_id: scope_id
                        .as_str()
                        .context("scope_association_scope_id")?
                        .into(),
                    module: module.into(),
                    rust_path: path.into(),
                    source_sha256: digest.clone(),
                    resolution: if digest.is_some() {
                        "declared_source_matched"
                    } else {
                        "declared_source_missing"
                    }
                    .into(),
                });
            }
        }
        implementations.sort_by(|a, b| {
            (&a.scope_id, &a.module, &a.rust_path).cmp(&(&b.scope_id, &b.module, &b.rust_path))
        });
        Ok(Self {
            audit,
            mappings_sha256,
            implementations,
        })
    }
}

/// No Deserialize or public constructor. Only the parent owner can construct
/// this after its fresh dispatch, retained raw-stream and current-input guards.
pub(super) struct VerifiedScopeObservation<'a> {
    report: &'a CodeScopeWitnessReport,
    reference: &'a ArtifactReference,
}
impl<'a> VerifiedScopeObservation<'a> {
    pub(super) fn from_owner(
        report: &'a CodeScopeWitnessReport,
        reference: &'a ArtifactReference,
    ) -> Result<Self> {
        ensure!(
            report.witness.execution == "completed"
                && report.behavior_status == "unknown"
                && report.eligibility == "unknown"
                && report.whole_code_cycle == "not_connected",
            "scope_association_observation_not_completed_unknown"
        );
        Ok(Self { report, reference })
    }
}

/// Structural total join over original identities. A join does not validate
/// execution; the parent owner supplies the separately guarded observation.
fn validate_item_join(audit: &Value, rows: &[CodeScopeItemEvidence]) -> Result<()> {
    let original = audit["items"]
        .as_array()
        .context("scope_association_original_items")?;
    let expected: BTreeSet<&str> = original
        .iter()
        .map(|row| row["id"].as_str().context("scope_association_original_id"))
        .collect::<Result<_>>()?;
    let actual: BTreeSet<&str> = rows.iter().map(|row| row.scope_id.as_str()).collect();
    ensure!(
        expected.len() == oe_evolution::scope_witness::DENOMINATOR
            && original.len() == expected.len()
            && actual == expected
            && rows.len() == expected.len()
            && rows.iter().all(|row| row.behavior_status == "unknown"),
        "scope_association_original_identity_set_changed"
    );
    Ok(())
}

/// Pure association, not a substitute for the owner's execution guards.
pub(super) fn assemble(
    snapshot: &CandidateSnapshot,
    request: &CodeArtifactEligibilityRequest,
    candidate: &CodeSourceArtifact,
    policy_sha256: &str,
    context: &ArtifactReference,
    qualification: &CodeArtifactEligibilityResult,
    observation: VerifiedScopeObservation<'_>,
) -> Result<CodeScopeAssociationIndex> {
    let report = observation.report;
    validate_item_join(&snapshot.audit, &report.items)?;
    ensure!(
        report.scope_sha256 == oe_evolution::scope_witness::SCOPE_SHA
            && report.scope_lock_sha256 == oe_evolution::scope_witness::LOCK_SHA
            && report.fixed_scope_count == oe_evolution::scope_witness::DENOMINATOR as u64,
        "scope_association_original_identity_set_changed"
    );
    ensure!(
        qualification.execution == "completed"
            && qualification.eligibility == "unknown"
            && qualification.artifact_sha256 == request.source.sha256
            && qualification.comparison_sha256 == request.assurance.sha256
            && report.witness.candidate_sha256 == candidate.manifest_sha256,
        "scope_association_subject_changed"
    );
    let mut items = report.items.clone();
    items.sort_by(|a, b| a.scope_id.cmp(&b.scope_id));
    let index = CodeScopeAssociationIndex {
        schema: "oev2.code-scope-association-index/1".into(),
        scope_sha256: report.scope_sha256.clone(),
        scope_lock_sha256: report.scope_lock_sha256.clone(),
        fixed_scope_count: report.fixed_scope_count,
        request: request.clone(),
        candidate_manifest_sha256: candidate.manifest_sha256.clone(),
        policy_sha256: policy_sha256.into(),
        context: context.clone(),
        candidate_mappings_sha256: snapshot.mappings_sha256.clone(),
        witness: observation.reference.clone(),
        items,
        implementations: snapshot.implementations.clone(),
        uncovered: qualification.uncovered.clone(),
        behavior_status: "unknown".into(),
        eligibility: "unknown".into(),
    };
    index
        .validate()
        .map_err(|errors| anyhow::anyhow!("scope_association_index_contract:{errors:?}"))?;
    Ok(index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn audit() -> Value {
        json!({"declared_mappings":[
            {"scope_ids":["id-b","id-a"],"module":"semantic_runtime","rust_path":"crates/owner.rs"},
            {"scope_ids":["id-a"],"module":"semantic_runtime","rust_path":"crates/missing.rs"}
        ]})
    }

    #[test]
    fn association_paths_use_candidate_manifest_and_keep_missing_declarations() {
        // Structural fixture only: no witness or execution qualification is minted.
        let candidate = json!({"files":{(MAPPINGS):{"sha256":"a".repeat(64)},
            "crates/owner.rs":{"sha256":"b".repeat(64)}}});
        let snapshot = CandidateSnapshot::from_audit(audit(), &candidate).unwrap();
        assert_eq!(
            snapshot.mappings_sha256.as_deref(),
            Some("a".repeat(64).as_str())
        );
        assert_eq!(snapshot.implementations.len(), 3);
        assert_eq!(snapshot.implementations[0].scope_id, "id-a");
        assert_eq!(snapshot.implementations[0].rust_path, "crates/missing.rs");
        assert_eq!(
            snapshot.implementations[0].resolution,
            "declared_source_missing"
        );
        assert_eq!(snapshot.implementations[0].source_sha256, None);
        assert!(
            snapshot
                .implementations
                .iter()
                .filter(|r| r.rust_path == "crates/owner.rs")
                .all(|r| r.source_sha256.as_deref() == Some("b".repeat(64).as_str()))
        );
        let foreign = json!({"files":{"crates/owner.rs":{"sha256":"c".repeat(64)}}});
        assert_ne!(
            snapshot,
            CandidateSnapshot::from_audit(audit(), &foreign).unwrap()
        );
    }

    #[test]
    fn association_missing_mapping_is_explicit_and_bad_digest_is_not_trusted() {
        let empty =
            CandidateSnapshot::from_audit(json!({"declared_mappings":[]}), &json!({"files":{}}))
                .unwrap();
        assert_eq!(empty.mappings_sha256, None);
        assert!(empty.implementations.is_empty());
        assert!(
            CandidateSnapshot::from_audit(
                audit(),
                &json!({"files":{"crates/owner.rs":{"sha256":"passed"}}})
            )
            .is_err()
        );
    }

    #[test]
    fn association_total_join_preserves_original_ids_and_rejects_omission_or_promotion() {
        // Read the actual immutable inventory; synthetic unknown rows test only
        // the total join and never enter the owner as a process witness.
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let audit = oe_evolution::scope_witness::inventory(root).unwrap();
        let rows: Vec<CodeScopeItemEvidence> = audit["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| CodeScopeItemEvidence {
                scope_id: item["id"].as_str().unwrap().into(),
                observed_case_ids: Vec::new(),
                behavior_status: "unknown".into(),
            })
            .collect();
        assert_eq!(rows.len(), 13494);
        validate_item_join(&audit, &rows).unwrap();
        let mut reversed = rows.clone();
        reversed.reverse();
        validate_item_join(&audit, &reversed).unwrap();
        assert!(validate_item_join(&audit, &rows[1..]).is_err());
        let mut duplicate = rows.clone();
        duplicate[0] = duplicate[1].clone();
        assert!(validate_item_join(&audit, &duplicate).is_err());
        let mut replaced = rows.clone();
        replaced[0].scope_id = "invented|abstract|approved".into();
        assert!(validate_item_join(&audit, &replaced).is_err());
        let mut promoted = rows;
        promoted[0].behavior_status = "approved_abstract".into();
        assert!(validate_item_join(&audit, &promoted).is_err());
    }
}
