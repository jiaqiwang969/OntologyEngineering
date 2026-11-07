//! Reconstruct historical review obligations from immutable archived bytes.
//!
//! Historical receipts retain their original execution identity. Checking their
//! complete source binding and aggregation here is not a new semantic execution
//! or current engineering acceptance. The workspace owner independently replays
//! the current head's reviews against current inputs before allowing continuation.

use anyhow::{Context, Result, bail, ensure};
use oe_contracts::{
    ArtifactReference, DecisionReviewReplay, ObjectIdentity, WorkspaceCommit, WorkspaceHead,
    WorkspaceResolution,
};
use oe_core::{SkillRoot, read_regular_bytes, sha256};
use oe_kernel::{
    Binding, Receipt,
    project::{self, Project},
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

const FILE_LIMIT: usize = 16 * 1024 * 1024;
/// Request/commit /1 through /5: 1024 commits and 64 MiB.
pub(crate) const LEGACY_HISTORY_BYTES: usize = 64 * 1024 * 1024;
/// Explicit request/commit /6: 1024 commits and 256 MiB.
pub(crate) const LARGE_HISTORY_BYTES: usize = 256 * 1024 * 1024;
pub(crate) const HISTORY_LIMIT: usize = 1024;
type ReviewCoverage = (BTreeSet<String>, BTreeSet<String>);

/// Budget of one complete history read: commit files counted from the head
/// itself and the archived Project and review bytes they bind. Older profiles
/// never inherit a larger budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HistoryBudget {
    pub commits: usize,
    pub commit_bytes: usize,
    pub archive_bytes: usize,
}

/// What one complete history read used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HistoryUse {
    pub commits: usize,
    pub commit_bytes: usize,
    pub archive_bytes: usize,
}

pub(crate) fn history_budget(schema: &str) -> Result<HistoryBudget> {
    let bytes = match schema {
        "oev2.workspace-commit-request/1"
        | "oev2.workspace-commit-request/2"
        | "oev2.workspace-commit-request/3"
        | "oev2.workspace-commit-request/4"
        | "oev2.workspace-commit-request/5" => LEGACY_HISTORY_BYTES,
        "oev2.workspace-commit-request/6" => LARGE_HISTORY_BYTES,
        _ => bail!("workspace_request_schema"),
    };
    Ok(HistoryBudget {
        commits: HISTORY_LIMIT,
        commit_bytes: bytes,
        archive_bytes: bytes,
    })
}

struct Archive<'a> {
    root: &'a SkillRoot,
    bytes: BTreeMap<String, Vec<u8>>,
    reviews: BTreeMap<(String, String), ReviewCoverage>,
    total: usize,
    limit: usize,
}
impl<'a> Archive<'a> {
    fn new(root: &'a SkillRoot, limit: usize) -> Self {
        Self {
            root,
            bytes: BTreeMap::new(),
            reviews: BTreeMap::new(),
            total: 0,
            limit,
        }
    }
    fn read(&mut self, reference: &ArtifactReference) -> Result<Vec<u8>> {
        reference
            .validate()
            .map_err(|_| anyhow::anyhow!("workspace_history_reference_invalid"))?;
        oe_semantic::safe_relative(&reference.path)?;
        if let Some(raw) = self.bytes.get(&reference.sha256) {
            return Ok(raw.clone());
        }
        let path = self
            .root
            .private_path(Path::new(&format!("artifacts/{}", reference.sha256)))?;
        let raw = read_regular_bytes(&path, FILE_LIMIT)?;
        ensure!(
            sha256(&raw) == reference.sha256,
            "workspace_history_archive_changed"
        );
        self.total = self
            .total
            .checked_add(raw.len())
            .context("workspace_history_archive_overflow")?;
        ensure!(self.total <= self.limit, "workspace_history_archive_budget");
        self.bytes.insert(reference.sha256.clone(), raw.clone());
        Ok(raw)
    }
    fn json(&mut self, reference: &ArtifactReference) -> Result<Value> {
        Ok(oe_core::json::strict_json(&self.read(reference)?)?)
    }
    fn project(&mut self, reference: &ArtifactReference) -> Result<Project> {
        let p: Project = serde_json::from_value(self.json(reference)?)?;
        project::validate(&p)?;
        Ok(p)
    }
    fn recheck(&self) -> Result<()> {
        for (digest, expected) in &self.bytes {
            let path = self
                .root
                .private_path(Path::new(&format!("artifacts/{digest}")))?;
            ensure!(
                read_regular_bytes(&path, FILE_LIMIT)? == *expected,
                "workspace_history_archive_changed_during_read"
            );
        }
        Ok(())
    }
}

fn identities(value: &Value) -> Result<BTreeSet<String>> {
    let mut out = BTreeSet::new();
    for item in value
        .as_array()
        .context("workspace_history_identity_list")?
    {
        let id = item.as_str().context("workspace_history_identity")?;
        ensure!(
            !id.is_empty() && out.insert(id.into()),
            "workspace_history_duplicate_identity"
        );
    }
    Ok(out)
}
fn impacted(before: &Project, current: &Project) -> Result<(Value, BTreeSet<String>)> {
    let report = project::reconcile(before, current)?;
    let ids = report["decisions"]
        .as_array()
        .context("workspace_history_impact")?
        .iter()
        .map(|d| {
            d["decision_id"]
                .as_str()
                .map(str::to_owned)
                .context("workspace_history_impact_identity")
        })
        .collect::<Result<_>>()?;
    Ok((report, ids))
}

/// An individually reviewed downstream decision remains pending while a
/// currently declared prerequisite still lacks its own qualifying review.
pub fn propagate_pending(project: &Project, pending: &mut BTreeSet<String>) -> Result<()> {
    let impact = project::decision_impact(project, pending)?;
    for decision in impact["decisions"]
        .as_array()
        .context("workspace_pending_impact_missing")?
    {
        pending.insert(
            decision["decision_id"]
                .as_str()
                .context("workspace_pending_decision_missing")?
                .into(),
        );
    }
    Ok(())
}

/// A saved review keeps the identity it ran under: a later release reads the record and does not
/// re-sign it, so its engine version is not compared with the running program.
fn saved_receipt_intact(value: &Receipt, binding_hash: &str, evidence_sha256: &str) -> Result<bool> {
    Ok(value.schema == "oev2.semantic-receipt/1"
        && value.engine_id == oe_kernel::ENGINE_ID
        && !value.engine_version.trim().is_empty()
        && value.execution == "completed"
        && value.conclusion == "pass"
        && value.failures.is_empty()
        && value.binding.digest()? == binding_hash
        && value.binding_sha256 == binding_hash
        && value.evidence_sha256 == evidence_sha256
        && value.executable_sha256.len() == 64
        && value
            .executable_sha256
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()))
}

fn receipt(
    archive: &mut Archive<'_>,
    p: &Project,
    entry: &DecisionReviewReplay,
    result: &Value,
) -> Result<()> {
    let decision = p
        .decisions
        .iter()
        .find(|d| d.id == entry.decision_id)
        .context("workspace_history_review_decision_missing")?;
    ensure!(
        decision.conclusion == "supported",
        "workspace_history_review_decision_unsupported"
    );
    let binding: Binding = serde_json::from_value(archive.json(&entry.binding)?)?;
    binding.validate()?;
    let object = ObjectIdentity {
        object_id: decision.object_id.clone(),
        revision: decision.object_revision.clone(),
        condition_scope: decision.condition_scope.clone(),
        source_sha256: decision.source_sha256.clone(),
    };
    ensure!(
        binding.project_id == p.project_id
            && binding.object_id == object.object_id
            && binding.object_revision == object.revision
            && binding.condition_scope == object.condition_scope
            && binding.source_sha256 == object.source_sha256
            && binding.project_context.as_ref() == Some(&project::context(p, &object)?),
        "workspace_history_review_context_changed"
    );
    let basis = oe_kernel::engineering::basis(p, &object)?
        .context("workspace_history_review_basis_missing")?;
    ensure!(
        !basis.assertions.is_empty() && !basis.acceptance.is_empty() && !entry.shape_ids.is_empty(),
        "workspace_history_review_obligations_missing"
    );
    ensure!(
        [
            "bound_focus_context",
            "current_project_context",
            "current_condition_model"
        ]
        .iter()
        .all(|id| binding.required_checks.contains(*id))
            && entry
                .shape_ids
                .iter()
                .chain(&entry.query_ids)
                .all(|id| binding.required_checks.contains(id)),
        "workspace_history_review_checks_missing"
    );
    let evidence = archive.read(&entry.evidence)?;
    let saved: Receipt = serde_json::from_value(archive.json(&entry.receipt)?)?;
    let observed: Receipt = serde_json::from_value(result.clone())?;
    let binding_hash = binding.digest()?;
    for value in [&saved, &observed] {
        ensure!(
            saved_receipt_intact(value, &binding_hash, &sha256(&evidence))?,
            "workspace_history_review_receipt_invalid"
        );
        ensure!(
            value
                .checks
                .iter()
                .all(|c| c.execution == "completed" && c.conclusion == "pass"),
            "workspace_history_review_aggregation_changed"
        );
    }
    ensure!(
        saved.engine_version == observed.engine_version
            && saved.executable_sha256 == observed.executable_sha256
            && saved.coverage == observed.coverage
            && json!(saved.checks) == json!(observed.checks),
        "workspace_history_review_receipt_mismatch"
    );
    // Re-run the one Rust aggregation owner over the frozen check records.
    // Its newly measured executable hash is deliberately not substituted for
    // the historical execution identity stored in the source receipt.
    let aggregate = oe_kernel::evaluate(saved.binding.clone(), &evidence, saved.checks.clone())?;
    ensure!(
        aggregate.execution == saved.execution
            && aggregate.conclusion == saved.conclusion
            && aggregate.coverage == saved.coverage
            && aggregate.failures == saved.failures,
        "workspace_history_review_aggregation_changed"
    );
    Ok(())
}

fn resolution(
    archive: &mut Archive<'_>,
    current_ref: &ArtifactReference,
    resolution: &WorkspaceResolution,
) -> Result<(BTreeSet<String>, BTreeSet<String>)> {
    let request = archive.json(&resolution.request)?;
    let result = archive.json(&resolution.result)?;
    ensure!(
        request["current"] == json!(current_ref) && result["current"] == json!(current_ref),
        "workspace_history_review_request_changed"
    );
    let key = (
        resolution.request.sha256.clone(),
        resolution.result.sha256.clone(),
    );
    if let Some(coverage) = archive.reviews.get(&key) {
        return Ok(coverage.clone());
    }
    let (before_ref, after_ref, sources, entries, result_schema) = match request["schema"].as_str()
    {
        Some("oev2.reviewed-reconciliation-request/1") => {
            let r: oe_contracts::ReviewedReconciliationRequest =
                serde_json::from_value(request.clone())?;
            r.validate()
                .map_err(|_| anyhow::anyhow!("workspace_history_review_contract"))?;
            (
                r.before,
                r.current,
                r.sources,
                r.reviews,
                "oev2.reviewed-reconciliation-result/1",
            )
        }
        Some("oev2.reviewed-reconciliation-request/2") => {
            let r: oe_contracts::ReviewedReconciliationRequest2 =
                serde_json::from_value(request.clone())?;
            r.validate()
                .map_err(|_| anyhow::anyhow!("workspace_history_review_contract"))?;
            (
                r.before,
                r.current,
                r.sources,
                r.reviews,
                "oev2.reviewed-reconciliation-result/2",
            )
        }
        _ => bail!("workspace_history_review_owner_invalid"),
    };
    ensure!(
        after_ref == *current_ref
            && result["current"] == json!(current_ref)
            && result["schema"] == result_schema
            && result["engineering_adoption"] == "not_granted"
            && result["request_sha256"] == sha256(oe_core::json::canonical_json(&request)?),
        "workspace_history_review_request_changed"
    );
    let before = archive.project(&before_ref)?;
    let current = archive.project(&after_ref)?;
    let (impact, affected) = impacted(&before, &current)?;
    ensure!(
        result["legacy_impact"] == impact,
        "workspace_history_review_impact_changed"
    );
    ensure!(
        sources.len() <= 4096 && entries.len() <= 32,
        "workspace_history_review_budget"
    );
    let mut paths = BTreeSet::new();
    let mut digests = BTreeSet::new();
    let mut bytes = 0usize;
    for source in &sources {
        ensure!(
            paths.insert(&source.path) && current.evidence.values().any(|h| h == &source.sha256),
            "workspace_history_review_source_invalid"
        );
        bytes = bytes
            .checked_add(archive.read(source)?.len())
            .context("workspace_history_review_byte_overflow")?;
        digests.insert(source.sha256.clone());
    }
    ensure!(
        current.evidence.values().all(|h| digests.contains(h)),
        "workspace_history_review_sources_incomplete"
    );
    if result_schema.ends_with("/2") {
        ensure!(
            result["source_inventory"]
                == json!({"files":sources.len(),"unique_digests":digests.len(),"bytes_per_pass":bytes,"verification_passes":2}),
            "workspace_history_review_inventory_changed"
        );
    }
    let rows = result["reviews"]
        .as_array()
        .context("workspace_history_review_rows")?;
    ensure!(
        rows.len() == entries.len(),
        "workspace_history_review_rows_incomplete"
    );
    let mut seen = BTreeSet::new();
    let mut passed = BTreeSet::new();
    for (entry, row) in entries.iter().zip(rows) {
        ensure!(
            seen.insert(entry.decision_id.clone())
                && affected.contains(&entry.decision_id)
                && row["decision_id"] == entry.decision_id,
            "workspace_history_review_row_identity"
        );
        // All leaf references remain bound even for a failed historical review.
        for reference in [&entry.binding, &entry.evidence, &entry.receipt] {
            archive.read(reference)?;
        }
        match row["status"].as_str() {
            Some("replayed_current") => {
                receipt(archive, &current, entry, &row["receipt"])?;
                passed.insert(entry.decision_id.clone());
            }
            Some("requires_review") => {}
            _ => bail!("workspace_history_review_row_status"),
        }
    }
    let mut dependencies = before.dependencies.clone();
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
            if values.iter().any(|v| pending.contains(v)) {
                pending.insert(id.clone());
            }
        }
        if pending == prior {
            break;
        }
    }
    ensure!(
        passed.is_disjoint(&pending),
        "workspace_history_review_prerequisite_pending"
    );
    let remaining: BTreeSet<_> = affected.difference(&passed).cloned().collect();
    ensure!(
        identities(&result["review_satisfied"])? == passed
            && identities(&result["still_requires_review"])? == remaining
            && result["status"]
                == if remaining.is_empty() {
                    "review_complete"
                } else {
                    "review_partial"
                },
        "workspace_history_review_coverage_changed"
    );
    let coverage = (passed, remaining);
    archive.reviews.insert(key, coverage.clone());
    Ok(coverage)
}

pub(crate) fn validate_pending(
    root: &SkillRoot,
    head: &WorkspaceHead,
    budget: HistoryBudget,
) -> Result<HistoryUse> {
    let mut next = Some(head.commit.clone());
    let mut expected_sequence = head.sequence;
    let mut chain = Vec::new();
    let mut references = Vec::new();
    let mut seen = BTreeSet::new();
    let mut total = 0usize;
    while let Some(reference) = next {
        ensure!(
            chain.len() < budget.commits && seen.insert(reference.sha256.clone()),
            "workspace_history_chain_budget_or_cycle"
        );
        let raw = crate::artifacts::read(root, &reference)?;
        total = total
            .checked_add(raw.len())
            .context("workspace_history_byte_overflow")?;
        ensure!(
            total <= budget.commit_bytes,
            "workspace_history_byte_budget"
        );
        let commit: WorkspaceCommit = serde_json::from_value(oe_core::json::strict_json(&raw)?)?;
        commit
            .validate()
            .map_err(|_| anyhow::anyhow!("workspace_history_commit_invalid"))?;
        commit
            .proposal
            .validate()
            .map_err(|_| anyhow::anyhow!("workspace_history_proposal_invalid"))?;
        ensure!(
            commit.sequence == expected_sequence
                && expected_sequence > 0
                && commit.proposal.workspace_id == head.workspace_id
                && reference.path == format!("commits/{}/commit.json", commit.proposal.commit_id),
            "workspace_history_commit_identity"
        );
        next = commit.proposal.expected_parent.clone();
        ensure!(
            (expected_sequence == 1) == next.is_none(),
            "workspace_history_parent_sequence"
        );
        expected_sequence -= 1;
        references.push(reference);
        chain.push(commit);
    }
    let mut archive = Archive::new(root, budget.archive_bytes);
    let mut previous: Option<Project> = None;
    let mut pending = BTreeSet::new();
    for commit in chain.iter().rev() {
        let current = archive.project(&commit.proposal.project)?;
        if let Some(before) = &previous {
            pending.extend(impacted(before, &current)?.1);
        }
        let mut resolutions = BTreeMap::new();
        for r in commit.proposal.reviews.iter().chain(
            commit
                .proposal
                .corrections
                .iter()
                .flat_map(|c| c.resolutions.iter()),
        ) {
            ensure!(
                resolutions
                    .insert(r.result.path.clone(), r)
                    .is_none_or(|before| before == r),
                "workspace_history_conflicting_review"
            );
        }
        for r in resolutions.values() {
            // Old resolutions may remain linked to a reopened correction. They
            // cannot clear obligations for a different Project snapshot.
            let request = archive.json(&r.request)?;
            let result = archive.json(&r.result)?;
            if request["current"] != json!(commit.proposal.project)
                || result["current"] != json!(commit.proposal.project)
            {
                continue;
            }
            let (satisfied, required) = resolution(&mut archive, &commit.proposal.project, r)?;
            for id in satisfied {
                pending.remove(&id);
            }
            pending.extend(required);
        }
        propagate_pending(&current, &mut pending)?;
        ensure!(
            commit.pending_decisions == pending.iter().cloned().collect::<Vec<_>>(),
            "workspace_history_pending_decisions_changed"
        );
        previous = Some(current);
    }
    archive.recheck()?;
    let used = HistoryUse {
        commits: references.len(),
        commit_bytes: total,
        archive_bytes: archive.total,
    };
    for reference in references {
        crate::artifacts::read(root, &reference)?;
    }
    Ok(used)
}

#[cfg(test)]
mod saved_receipt_tests {
    use super::*;

    fn receipt() -> (Receipt, String) {
        let binding: Binding = serde_json::from_value(json!({
            "project_id":"p","object_id":"o","object_revision":"r1","package_id":"b","package_version":"1",
            "package_sha256":"a".repeat(64),"source_sha256":"b".repeat(64),"condition_scope":"c",
            "required_checks":["bound_focus_context"],"authority_id":"a"
        }))
        .unwrap();
        let hash = binding.digest().unwrap();
        (
            Receipt {
                schema: "oev2.semantic-receipt/1".into(),
                engine_id: oe_kernel::ENGINE_ID.into(),
                engine_version: "2.0.0-rc.1".into(),
                executable_sha256: "c".repeat(64),
                binding,
                binding_sha256: hash.clone(),
                evidence_sha256: "d".repeat(64),
                execution: "completed".into(),
                conclusion: "pass".into(),
                coverage: BTreeSet::new(),
                failures: vec![],
                adoption_conditions: vec![],
                checks: vec![],
            },
            hash,
        )
    }

    #[test]
    fn an_older_engine_version_stays_valid_but_a_broken_identity_does_not() {
        let evidence = "d".repeat(64);
        let (good, hash) = receipt();
        assert_ne!(good.engine_version, oe_kernel::ENGINE_VERSION);
        assert!(saved_receipt_intact(&good, &hash, &evidence).unwrap());
        let changes: [fn(&mut Receipt); 5] = [
            |r| r.engine_version = " ".into(),
            |r| r.engine_id = "another-engine".into(),
            |r| r.conclusion = "unknown".into(),
            |r| r.binding_sha256 = "e".repeat(64),
            |r| r.executable_sha256 = "short".into(),
        ];
        for change in changes {
            let mut r = good.clone();
            change(&mut r);
            assert!(!saved_receipt_intact(&r, &hash, &evidence).unwrap());
        }
    }
}
