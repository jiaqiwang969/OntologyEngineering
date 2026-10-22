//! Original change impact plus freshly replayed review. Does not grant adoption.
use crate::artifacts;
use anyhow::{Context, Result, ensure};
use oe_contracts::{
    ArtifactReference, DecisionReviewReplay, FailureRecord, ObjectIdentity, ReviewRequest,
    ReviewedReconciliationPort, ReviewedReconciliationPort2, ReviewedReconciliationRequest,
    ReviewedReconciliationRequest2, ReviewedReconciliationResult, ReviewedReconciliationResult2,
    ReviewedSourceBudget2, ReviewedSourceObservation2,
};
use oe_core::{SkillRoot, sha256};
use oe_kernel::{
    Binding, Receipt,
    port::RustSemanticPort,
    project::{self, Project},
};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

pub struct ReviewedReconciliationExecutor<'a> {
    pub root: &'a SkillRoot,
}
impl ReviewedReconciliationPort for ReviewedReconciliationExecutor<'_> {
    fn reconcile_reviewed(
        &self,
        input: &ReviewedReconciliationRequest,
    ) -> std::result::Result<ReviewedReconciliationResult, FailureRecord> {
        reconcile_reviewed(self.root, input)
            .map_err(|e| artifacts::failure("reviewed_reconciliation", e))
    }
}
impl ReviewedReconciliationPort2 for ReviewedReconciliationExecutor<'_> {
    fn reconcile_reviewed2(
        &self,
        input: &ReviewedReconciliationRequest2,
    ) -> std::result::Result<ReviewedReconciliationResult2, FailureRecord> {
        reconcile_reviewed2(self.root, input)
            .map_err(|e| artifacts::failure("reviewed_reconciliation2", e))
    }
}

#[derive(Debug, PartialEq, Eq)]
struct InventoryObservation {
    files: u64,
    unique_digests: u64,
    bytes: u64,
}

fn source_inventory(
    root: &SkillRoot,
    p: &Project,
    sources: &[ArtifactReference],
) -> Result<InventoryObservation> {
    ensure!(sources.len() <= 256, "reviewed_source_limit");
    let mut paths = BTreeSet::new();
    let mut hashes = BTreeSet::new();
    let mut total = 0usize;
    for r in sources {
        ensure!(paths.insert(&r.path), "reviewed_duplicate_source_path");
        ensure!(
            p.evidence.values().any(|h| h == &r.sha256),
            "reviewed_source_not_declared"
        );
        let bytes = artifacts::read(root, r)?;
        total = total
            .checked_add(bytes.len())
            .context("reviewed_source_size_overflow")?;
        ensure!(total <= 64 * 1024 * 1024, "reviewed_total_source_limit");
        hashes.insert(r.sha256.clone());
    }
    ensure!(
        p.evidence.values().all(|h| hashes.contains(h)),
        "reviewed_source_inventory_incomplete"
    );
    Ok(InventoryObservation {
        files: sources.len() as u64,
        unique_digests: hashes.len() as u64,
        bytes: total as u64,
    })
}

fn validate_source_budget(budget: &ReviewedSourceBudget2) -> Result<()> {
    budget
        .validate()
        .map_err(|_| anyhow::anyhow!("reviewed_source_budget_contract"))?;
    ensure!(
        (1..=4096).contains(&budget.max_files)
            && (1..=256 * 1024 * 1024).contains(&budget.max_file_bytes)
            && (1..=2 * 1024 * 1024 * 1024).contains(&budget.max_total_bytes_per_pass),
        "reviewed_source_budget_out_of_range"
    );
    Ok(())
}

fn streamed_source_inventory(
    root: &SkillRoot,
    p: &Project,
    sources: &[ArtifactReference],
    budget: &ReviewedSourceBudget2,
) -> Result<InventoryObservation> {
    validate_source_budget(budget)?;
    ensure!(
        sources.len() as u64 <= budget.max_files,
        "reviewed_source_count_budget"
    );
    let declared: BTreeSet<_> = p.evidence.values().collect();
    let mut paths = BTreeSet::new();
    for reference in sources {
        reference
            .validate()
            .map_err(|_| anyhow::anyhow!("artifact_contract_invalid"))?;
        oe_semantic::safe_relative(&reference.path)?;
        ensure!(
            paths.insert(&reference.path),
            "reviewed_duplicate_source_path"
        );
        ensure!(
            declared.contains(&reference.sha256),
            "reviewed_source_not_declared"
        );
    }
    let mut hashes = BTreeSet::new();
    let mut total = 0u64;
    for reference in sources {
        let remaining = budget
            .max_total_bytes_per_pass
            .checked_sub(total)
            .context("reviewed_source_total_budget")?;
        // Same regular-file, platform and streaming fingerprint owner. No
        // JSON/parser limit is raised, and source binaries are never executed.
        let identity = oe_core::fingerprint_file(
            &root.path().join(&reference.path),
            budget.max_file_bytes.min(remaining),
        )
        .context("reviewed_stream_source_read")?;
        ensure!(
            identity.sha256 == reference.sha256,
            "artifact_source_changed"
        );
        total = total
            .checked_add(identity.bytes)
            .context("reviewed_source_size_overflow")?;
        ensure!(
            total <= budget.max_total_bytes_per_pass,
            "reviewed_source_total_budget"
        );
        hashes.insert(reference.sha256.clone());
    }
    ensure!(
        declared.iter().all(|h| hashes.contains(*h)),
        "reviewed_source_inventory_incomplete"
    );
    Ok(InventoryObservation {
        files: sources.len() as u64,
        unique_digests: hashes.len() as u64,
        bytes: total,
    })
}

fn replay(root: &SkillRoot, p: &Project, entry: &DecisionReviewReplay) -> Result<Receipt> {
    let d = p
        .decisions
        .iter()
        .find(|d| d.id == entry.decision_id)
        .context("reviewed_decision_missing")?;
    ensure!(
        d.conclusion == "supported",
        "reviewed_unknown_or_refuted_decision"
    );
    let binding: Binding = serde_json::from_value(artifacts::json(root, &entry.binding)?)?;
    binding.validate()?;
    ensure!(
        binding.project_id == p.project_id
            && binding.object_id == d.object_id
            && binding.object_revision == d.object_revision
            && binding.condition_scope == d.condition_scope
            && binding.source_sha256 == d.source_sha256,
        "reviewed_decision_binding_mismatch"
    );
    let object = ObjectIdentity {
        object_id: d.object_id.clone(),
        revision: d.object_revision.clone(),
        condition_scope: d.condition_scope.clone(),
        source_sha256: d.source_sha256.clone(),
    };
    ensure!(
        binding.project_context.as_ref() == Some(&project::context(p, &object)?),
        "reviewed_context_changed"
    );
    let basis = oe_kernel::engineering::basis(p, &object)?.context("reviewed_basis_missing")?;
    ensure!(
        !basis.assertions.is_empty() && !basis.acceptance.is_empty(),
        "reviewed_obligations_empty"
    );
    ensure!(!entry.shape_ids.is_empty(), "reviewed_shape_required");
    let required = binding.required_checks.clone();
    ensure!(
        [
            "bound_focus_context",
            "current_project_context",
            "current_condition_model"
        ]
        .iter()
        .all(|id| required.contains(*id))
            && oe_kernel::engineering::checks_current(root, p, &object)
                .iter()
                .all(|c| required.contains(&c.id))
            && entry
                .shape_ids
                .iter()
                .chain(&entry.query_ids)
                .all(|id| required.contains(id)),
        "reviewed_required_checks_missing"
    );
    let evidence = artifacts::read(root, &entry.evidence)?;
    let saved: Receipt = serde_json::from_value(artifacts::json(root, &entry.receipt)?)?;
    ensure!(
        saved.schema == "oev2.semantic-receipt/1"
            && saved.engine_id == oe_kernel::ENGINE_ID
            && saved.engine_version == oe_kernel::ENGINE_VERSION
            && saved.execution == "completed"
            && saved.conclusion == "pass"
            && saved.failures.is_empty()
            && saved
                .checks
                .iter()
                .all(|c| c.execution == "completed" && c.conclusion == "pass")
            && saved.binding.digest()? == binding.digest()?
            && saved.binding_sha256 == binding.digest()?
            && saved.evidence_sha256 == sha256(&evidence),
        "reviewed_saved_receipt_invalid"
    );
    let request = ReviewRequest {
        object,
        evidence_sha256: sha256(&evidence),
        required_checks: required.into_iter().collect(),
    };
    let port = RustSemanticPort {
        root: root.path().to_owned(),
        binding,
        bundle: entry.bundle.clone(),
        queries: entry.query_ids.clone(),
        shapes: entry.shape_ids.clone(),
        evidence: evidence.clone(),
    };
    let historical = port.execute(&request, None)?;
    let fresh = project::review_current(root, p, &evidence, historical)?;
    ensure!(
        fresh.execution == "completed"
            && fresh.conclusion == "pass"
            && fresh.failures.is_empty()
            && fresh
                .checks
                .iter()
                .all(|c| c.execution == "completed" && c.conclusion == "pass"),
        "reviewed_fresh_review_not_passed"
    );
    // Exact current executor and complete check results: a saved status is not trusted.
    ensure!(
        saved.executable_sha256 == fresh.executable_sha256
            && saved.coverage == fresh.coverage
            && serde_json::to_value(&saved.checks)? == serde_json::to_value(&fresh.checks)?,
        "reviewed_receipt_replay_mismatch"
    );
    Ok(fresh)
}
fn recheck_inputs(
    root: &SkillRoot,
    before: &ArtifactReference,
    current: &ArtifactReference,
    reviews: &[DecisionReviewReplay],
) -> Result<()> {
    artifacts::read(root, before)?;
    artifacts::read(root, current)?;
    for r in reviews {
        for reference in [&r.binding, &r.evidence, &r.receipt] {
            artifacts::read(root, reference)?;
        }
    }
    Ok(())
}
pub fn reconcile_reviewed(
    root: &SkillRoot,
    input: &ReviewedReconciliationRequest,
) -> Result<ReviewedReconciliationResult> {
    input
        .validate()
        .map_err(|_| anyhow::anyhow!("reviewed_contract_invalid"))?;
    ensure!(
        input.schema == "oev2.reviewed-reconciliation-request/1" && input.reviews.len() <= 32,
        "reviewed_request_limit_or_schema"
    );
    let (outcome, _) = reconcile_parts(root, &input.before, &input.current, &input.reviews, |p| {
        source_inventory(root, p, &input.sources)
    })?;
    Ok(ReviewedReconciliationResult {
        schema: "oev2.reviewed-reconciliation-result/1".into(),
        request_sha256: sha256(oe_core::json::canonical_json(&serde_json::to_value(
            input,
        )?)?),
        current: input.current.clone(),
        legacy_impact: outcome.impact,
        reviews: outcome.rows,
        review_satisfied: outcome.satisfied,
        still_requires_review: outcome.pending,
        status: outcome.status,
        engineering_adoption: "not_granted".into(),
    })
}

pub fn reconcile_reviewed2(
    root: &SkillRoot,
    input: &ReviewedReconciliationRequest2,
) -> Result<ReviewedReconciliationResult2> {
    input
        .validate()
        .map_err(|_| anyhow::anyhow!("reviewed_contract_invalid"))?;
    ensure!(
        input.schema == "oev2.reviewed-reconciliation-request/2" && input.reviews.len() <= 32,
        "reviewed_request_limit_or_schema"
    );
    validate_source_budget(&input.source_budget)?;
    let (outcome, observation) =
        reconcile_parts(root, &input.before, &input.current, &input.reviews, |p| {
            streamed_source_inventory(root, p, &input.sources, &input.source_budget)
        })?;
    Ok(ReviewedReconciliationResult2 {
        schema: "oev2.reviewed-reconciliation-result/2".into(),
        request_sha256: sha256(oe_core::json::canonical_json(&serde_json::to_value(
            input,
        )?)?),
        current: input.current.clone(),
        legacy_impact: outcome.impact,
        reviews: outcome.rows,
        review_satisfied: outcome.satisfied,
        still_requires_review: outcome.pending,
        status: outcome.status,
        engineering_adoption: "not_granted".into(),
        source_inventory: ReviewedSourceObservation2 {
            files: observation.files,
            unique_digests: observation.unique_digests,
            bytes_per_pass: observation.bytes,
            verification_passes: 2,
        },
    })
}

struct ReplayOutcome {
    impact: serde_json::Value,
    rows: Vec<serde_json::Value>,
    satisfied: Vec<String>,
    pending: Vec<String>,
    status: String,
}

fn reconcile_parts(
    root: &SkillRoot,
    before_ref: &ArtifactReference,
    current_ref: &ArtifactReference,
    reviews: &[DecisionReviewReplay],
    mut verify_inventory: impl FnMut(&Project) -> Result<InventoryObservation>,
) -> Result<(ReplayOutcome, InventoryObservation)> {
    let before: Project = serde_json::from_value(artifacts::json(root, before_ref)?)?;
    let current: Project = serde_json::from_value(artifacts::json(root, current_ref)?)?;
    let impact = project::reconcile(&before, &current)?;
    let source_before = verify_inventory(&current)?;
    let packages: Vec<_> = reviews
        .iter()
        .map(|r| oe_semantic::load_bundle(root.path(), &r.bundle).map(|p| p.spec))
        .collect::<Result<_>>()?;
    let affected: BTreeSet<String> = impact["decisions"]
        .as_array()
        .context("reviewed_impact_invalid")?
        .iter()
        .map(|d| {
            d["decision_id"]
                .as_str()
                .map(str::to_owned)
                .context("reviewed_impact_id_missing")
        })
        .collect::<Result<_>>()?;
    let mut seen = BTreeSet::new();
    let mut passed = BTreeSet::new();
    let mut rows = Vec::new();
    for entry in reviews {
        ensure!(
            seen.insert(&entry.decision_id),
            "reviewed_duplicate_decision"
        );
        ensure!(
            affected.contains(&entry.decision_id),
            "reviewed_decision_not_affected"
        );
        match replay(root,&current,entry) {
            Ok(receipt)=>{passed.insert(entry.decision_id.clone());rows.push(json!({"decision_id":entry.decision_id,"status":"replayed_current","receipt":receipt}));},
            Err(error)=>rows.push(json!({"decision_id":entry.decision_id,"status":"requires_review","reason":error.to_string()})),
        }
    }
    // Preserve both prior and current declared dependency edges; removal cannot hide impact.
    let mut dependencies: BTreeMap<String, BTreeSet<String>> = before.dependencies.clone();
    for (id, values) in &current.dependencies {
        dependencies
            .entry(id.clone())
            .or_default()
            .extend(values.iter().cloned());
    }
    for d in before.decisions.iter().chain(&current.decisions) {
        dependencies
            .entry(d.id.clone())
            .or_default()
            .extend(d.basis.iter().cloned());
    }
    let mut pending: BTreeSet<_> = affected.difference(&passed).cloned().collect();
    loop {
        let prior = pending.clone();
        for (id, values) in &dependencies {
            if values.iter().any(|id| pending.contains(id)) {
                pending.insert(id.clone());
            }
        }
        if prior == pending {
            break;
        }
    }
    for row in &mut rows {
        let id = row["decision_id"].as_str().unwrap();
        if passed.contains(id) && pending.contains(id) {
            row["status"] = json!("requires_review");
            row["reason"] = json!("affected_prerequisite_pending");
        }
    }
    passed = passed.difference(&pending).cloned().collect();
    let pending: Vec<_> = affected.difference(&passed).cloned().collect();
    recheck_inputs(root, before_ref, current_ref, reviews)?;
    for (entry, prior) in reviews.iter().zip(packages) {
        ensure!(
            oe_semantic::load_bundle(root.path(), &entry.bundle)?.spec == prior,
            "reviewed_package_changed"
        );
    }
    let source_after = verify_inventory(&current)?;
    ensure!(
        source_before == source_after,
        "reviewed_source_inventory_changed"
    );
    Ok((
        ReplayOutcome {
            impact,
            rows,
            satisfied: passed.into_iter().collect(),
            status: if pending.is_empty() {
                "review_complete"
            } else {
                "review_partial"
            }
            .into(),
            pending,
        },
        source_after,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_change_between_complete_passes_cannot_return_result() {
        let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let scratch = workspace.join("var/builds");
        oe_core::ensure_directory(&scratch).unwrap();
        let dir = tempfile::tempdir_in(scratch.canonicalize().unwrap()).unwrap();
        std::fs::write(
            dir.path().join("SKILL.md"),
            "private synthetic source recheck",
        )
        .unwrap();
        std::fs::create_dir(dir.path().join("var")).unwrap();
        let source_path = dir.path().join("var/source.bin");
        std::fs::write(&source_path, b"original").unwrap();
        let reference = ArtifactReference {
            path: "var/source.bin".into(),
            sha256: sha256(b"original"),
        };
        let project_bytes = serde_json::to_vec(&json!({
            "schema":"oev2.project/1", "project_id":"synthetic-second-pass", "revision":"r1",
            "purpose":"Verify source recheck only, no engineering conclusion",
            "objects":{}, "conditions":{}, "evidence":{"original":reference.sha256},
            "decisions":[], "dependencies":{}
        }))
        .unwrap();
        std::fs::write(dir.path().join("var/project.json"), &project_bytes).unwrap();
        let project_ref = ArtifactReference {
            path: "var/project.json".into(),
            sha256: sha256(&project_bytes),
        };
        let root = SkillRoot::new(dir.path()).unwrap();
        let sources = vec![reference];
        let budget = ReviewedSourceBudget2 {
            max_files: 1,
            max_file_bytes: 64,
            max_total_bytes_per_pass: 64,
        };
        let mut completed_passes = 0;
        let result = reconcile_parts(&root, &project_ref, &project_ref, &[], |p| {
            let observation = streamed_source_inventory(&root, p, &sources, &budget)?;
            completed_passes += 1;
            if completed_passes == 1 {
                std::fs::write(&source_path, b"changed!").unwrap();
            }
            Ok(observation)
        });
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().to_string(), "artifact_source_changed");
        assert_eq!(completed_passes, 1);
        assert_eq!(
            std::fs::read(dir.path().join("var/project.json")).unwrap(),
            project_bytes
        );
    }
}
