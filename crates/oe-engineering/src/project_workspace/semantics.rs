//! Project-local continuity checks over material read and replayed by the owner.
//!
//! These checks preserve the existing Project, route and experience protocols.
//! Matching source bytes does not authenticate a speaker, establish a causal
//! claim, authorize execution, or grant engineering adoption. In particular,
//! `replayed_reviews` must come from the existing reconciliation owner; it must
//! never be populated by deserializing a caller's assertion of verification.

use anyhow::{Context, Result, ensure};
use oe_contracts::{
    ArtifactReference, WorkspaceCausalClaim, WorkspaceCorrectionObligation, WorkspaceGoal,
    WorkspaceLearningBinding, WorkspaceRequestSource, WorkspaceResolution,
};
use oe_kernel::project::{self, Project};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

const CAUSAL_LEVELS: [&str; 4] = [
    "sequence",
    "attribution",
    "mechanism",
    "controlled_comparison",
];

fn same_reference(left: &ArtifactReference, right: &ArtifactReference) -> bool {
    left.path == right.path && left.sha256 == right.sha256
}

fn reference_value(value: &Value, expected: &ArtifactReference) -> Result<bool> {
    let actual: ArtifactReference =
        serde_json::from_value(value.clone()).context("workspace_reference_invalid")?;
    Ok(same_reference(&actual, expected))
}

fn source<'a>(
    reference: &ArtifactReference,
    sources: &'a BTreeMap<String, Vec<u8>>,
) -> Result<&'a [u8]> {
    reference
        .validate()
        .map_err(|_| anyhow::anyhow!("workspace_source_reference_invalid"))?;
    let bytes = sources
        .get(&reference.path)
        .context("workspace_source_material_missing")?;
    ensure!(
        oe_core::sha256(bytes) == reference.sha256,
        "workspace_source_material_changed"
    );
    Ok(bytes)
}

fn json_source(
    reference: &ArtifactReference,
    sources: &BTreeMap<String, Vec<u8>>,
) -> Result<Value> {
    Ok(oe_core::json::strict_json(source(reference, sources)?)?)
}

fn identity(value: &Value) -> Result<&str> {
    let id = value.as_str().context("workspace_identity_missing")?;
    ensure!(!id.trim().is_empty(), "workspace_identity_missing");
    Ok(id)
}

fn identities(value: &Value) -> Result<BTreeSet<String>> {
    let mut ids = BTreeSet::new();
    for value in value
        .as_array()
        .context("workspace_identity_list_missing")?
    {
        let id = identity(value)?;
        ensure!(ids.insert(id.to_owned()), "workspace_duplicate_identity");
    }
    Ok(ids)
}

fn anchor_acceptance(anchor: &Value) -> Result<Vec<String>> {
    ensure!(
        matches!(
            anchor["schema"].as_str(),
            Some("oev2.route-anchor/1" | "oev2.route-anchor/2")
        ),
        "workspace_anchor_schema_invalid"
    );
    let acceptance = anchor["acceptance"]
        .as_array()
        .context("workspace_acceptance_missing")?;
    ensure!(!acceptance.is_empty(), "workspace_acceptance_missing");
    let mut seen = BTreeSet::new();
    acceptance
        .iter()
        .map(|item| {
            let id = identity(&item["id"])?;
            ensure!(seen.insert(id), "workspace_duplicate_acceptance");
            Ok(id.to_owned())
        })
        .collect()
}

/// Check the complete current source set and explicit correction obligations.
///
/// The owner reads ordinary files, checks paths/limits and obtains fresh native
/// reconciliation results before calling this function. All maps use the exact
/// ArtifactReference.path; this layer rechecks byte hashes and relationships.
#[allow(clippy::too_many_arguments)]
pub fn validate_semantics(
    project: &Project,
    project_ref: &ArtifactReference,
    anchor: &Value,
    requests: &[WorkspaceRequestSource],
    obligations: &[WorkspaceCorrectionObligation],
    causal_claims: &[WorkspaceCausalClaim],
    source_bytes: &BTreeMap<String, Vec<u8>>,
    replayed_reviews: &BTreeMap<String, Value>,
) -> Result<()> {
    project::validate(project)?;
    ensure!(
        json_source(project_ref, source_bytes)? == serde_json::to_value(project)?,
        "workspace_project_material_mismatch"
    );
    ensure!(
        anchor["project_id"].as_str() == Some(project.project_id.as_str()),
        "workspace_anchor_project_mismatch"
    );
    let acceptance: BTreeSet<_> = anchor_acceptance(anchor)?.into_iter().collect();
    let utterances = anchor["requests"]
        .as_array()
        .context("workspace_requests_missing")?;
    ensure!(
        utterances.first().is_some_and(|u| u["kind"] == "root"),
        "workspace_root_request_missing"
    );
    let mut original = BTreeMap::new();
    for utterance in utterances {
        let id = identity(&utterance["id"])?;
        ensure!(
            original.insert(id, utterance).is_none(),
            "workspace_duplicate_request"
        );
        ensure!(
            matches!(
                utterance["kind"].as_str(),
                Some("root" | "correction" | "approval")
            ),
            "workspace_request_kind_invalid"
        );
    }
    let mut recorded = BTreeSet::new();
    for request in requests {
        request
            .validate()
            .map_err(|_| anyhow::anyhow!("workspace_request_source_invalid"))?;
        ensure!(
            recorded.insert(request.request_id.as_str()),
            "workspace_duplicate_request_source"
        );
        let utterance = original
            .get(request.request_id.as_str())
            .context("workspace_request_source_unbound")?;
        let text = utterance["text"]
            .as_str()
            .context("workspace_request_text_missing")?;
        ensure!(!text.is_empty(), "workspace_request_text_missing");
        // Complete UTF-8 identity, not substring presence in a transcript and
        // not an interpretation of an assistant statement as user authority.
        ensure!(
            source(&request.source, source_bytes)? == text.as_bytes(),
            "workspace_request_text_mismatch"
        );
    }
    ensure!(
        recorded == original.keys().copied().collect(),
        "workspace_request_sources_incomplete"
    );

    let decision_ids: BTreeSet<_> = project.decisions.iter().map(|d| d.id.as_str()).collect();
    let mut obligation_ids = BTreeSet::new();
    let mut handled_corrections = BTreeSet::new();
    for obligation in obligations {
        obligation
            .validate()
            .map_err(|_| anyhow::anyhow!("workspace_correction_invalid"))?;
        ensure!(
            !obligation.id.trim().is_empty() && obligation_ids.insert(obligation.id.as_str()),
            "workspace_duplicate_correction_obligation"
        );
        ensure!(
            handled_corrections.insert(obligation.request_id.as_str()),
            "workspace_duplicate_correction_request"
        );
        let request = original
            .get(obligation.request_id.as_str())
            .context("workspace_correction_request_missing")?;
        ensure!(
            request["kind"] == "correction",
            "workspace_request_is_not_correction"
        );
        let affected_acceptance = identities(&request["affects"])?;
        ensure!(
            !affected_acceptance.is_empty() && affected_acceptance.is_subset(&acceptance),
            "workspace_correction_acceptance_invalid"
        );
        let affected: BTreeSet<_> = obligation
            .affected_decisions
            .iter()
            .map(String::as_str)
            .collect();
        ensure!(
            affected.len() == obligation.affected_decisions.len()
                && affected.is_subset(&decision_ids),
            "workspace_correction_decision_invalid"
        );
        ensure!(
            matches!(
                obligation.status.as_str(),
                "acknowledged" | "pending_review" | "resolved" | "deferred"
            ),
            "workspace_correction_status_invalid"
        );
        if matches!(obligation.status.as_str(), "resolved" | "deferred") {
            ensure!(
                !obligation.reason.trim().is_empty(),
                "workspace_correction_reason_missing"
            );
        }
        if obligation.status != "resolved" {
            // Historic resolution artifacts may remain linked when a change
            // reopens the obligation. They do not close its current state.
            for resolution in &obligation.resolutions {
                source(&resolution.request, source_bytes)?;
                source(&resolution.result, source_bytes)?;
            }
            continue;
        }
        ensure!(
            !affected.is_empty() && !obligation.resolutions.is_empty(),
            "workspace_correction_resolution_missing"
        );
        let mut acceptance_decisions = BTreeSet::new();
        for item in anchor["acceptance"].as_array().unwrap() {
            if affected_acceptance.contains(identity(&item["id"])?) {
                // Only /2 declares decision linkage. A /1 anchor can retain
                // the correction, but cannot claim it resolved in this layer.
                let declared = identities(&item["decisions"])
                    .context("workspace_correction_decision_link_missing")?;
                ensure!(
                    !declared.is_empty(),
                    "workspace_correction_decision_link_missing"
                );
                acceptance_decisions.extend(declared);
            }
        }
        ensure!(
            acceptance_decisions
                .iter()
                .all(|id| affected.contains(id.as_str())),
            "workspace_correction_affected_decisions_incomplete"
        );
        let mut satisfied = BTreeSet::new();
        let mut resolution_ids = BTreeSet::new();
        for resolution in &obligation.resolutions {
            ensure!(
                resolution_ids.insert((&resolution.request.path, &resolution.result.path)),
                "workspace_duplicate_resolution"
            );
            satisfied.extend(validate_resolution(
                project,
                project_ref,
                resolution,
                source_bytes,
                replayed_reviews,
            )?);
        }
        ensure!(
            affected.iter().all(|id| satisfied.contains(*id)),
            "workspace_correction_review_incomplete"
        );
    }
    let expected_corrections: BTreeSet<_> = original
        .iter()
        .filter(|(_, request)| request["kind"] == "correction")
        .map(|(id, _)| *id)
        .collect();
    ensure!(
        handled_corrections == expected_corrections,
        "workspace_correction_obligations_incomplete"
    );

    let mut claims = BTreeSet::new();
    for claim in causal_claims {
        claim
            .validate()
            .map_err(|_| anyhow::anyhow!("workspace_causal_claim_invalid"))?;
        ensure!(
            !claim.id.trim().is_empty() && claims.insert(claim.id.as_str()),
            "workspace_duplicate_causal_claim"
        );
        ensure!(
            !claim.statement.trim().is_empty()
                && CAUSAL_LEVELS.contains(&claim.level.as_str())
                && !claim.source_refs.is_empty(),
            "workspace_causal_claim_invalid"
        );
        let mut refs = BTreeSet::new();
        for reference in &claim.source_refs {
            ensure!(
                refs.insert((&reference.path, &reference.sha256)),
                "workspace_duplicate_causal_source"
            );
            source(reference, source_bytes)?;
        }
        ensure!(
            claim
                .confounders
                .iter()
                .chain(&claim.unknowns)
                .all(|s| !s.trim().is_empty()),
            "workspace_causal_gap_empty"
        );
    }
    Ok(())
}

fn validate_resolution(
    project: &Project,
    project_ref: &ArtifactReference,
    resolution: &WorkspaceResolution,
    source_bytes: &BTreeMap<String, Vec<u8>>,
    replayed_reviews: &BTreeMap<String, Value>,
) -> Result<BTreeSet<String>> {
    let request = json_source(&resolution.request, source_bytes)?;
    let result = json_source(&resolution.result, source_bytes)?;
    let expected_schema = match request["schema"].as_str() {
        Some("oev2.reviewed-reconciliation-request/1") => "oev2.reviewed-reconciliation-result/1",
        Some("oev2.reviewed-reconciliation-request/2") => "oev2.reviewed-reconciliation-result/2",
        _ => anyhow::bail!("workspace_resolution_owner_unsupported"),
    };
    ensure!(
        result["schema"] == expected_schema
            && result["engineering_adoption"] == "not_granted"
            && matches!(
                result["status"].as_str(),
                Some("review_complete" | "review_partial")
            ),
        "workspace_resolution_result_invalid"
    );
    ensure!(
        reference_value(&request["current"], project_ref)?
            && reference_value(&result["current"], project_ref)?
            && result["legacy_impact"]["project_id"] == project.project_id,
        "workspace_resolution_project_changed"
    );
    ensure!(
        result["request_sha256"] == oe_core::sha256(oe_core::json::canonical_json(&request)?),
        "workspace_resolution_request_changed"
    );
    let replayed = replayed_reviews
        .get(&resolution.result.path)
        .context("workspace_resolution_native_replay_missing")?;
    ensure!(*replayed == result, "workspace_resolution_replay_mismatch");
    let satisfied = identities(&result["review_satisfied"])?;
    let pending = identities(&result["still_requires_review"])?;
    ensure!(
        satisfied.is_disjoint(&pending),
        "workspace_resolution_coverage_conflict"
    );
    let rows = result["reviews"]
        .as_array()
        .context("workspace_resolution_reviews_missing")?;
    for id in &satisfied {
        let matching: Vec<_> = rows
            .iter()
            .filter(|row| row["decision_id"] == *id)
            .collect();
        ensure!(
            matching.len() == 1
                && matching[0]["status"] == "replayed_current"
                && matching[0]["receipt"]["execution"] == "completed"
                && matching[0]["receipt"]["conclusion"] == "pass",
            "workspace_resolution_actual_review_missing"
        );
    }
    Ok(satisfied)
}

/// Bind a new trial goal to the complete current Project and route anchor.
/// The caller has read and checked both exact referenced original files.
pub fn validate_learning_goal(
    goal: &WorkspaceGoal,
    project_ref: &ArtifactReference,
    anchor_ref: &ArtifactReference,
    anchor: &Value,
) -> Result<()> {
    goal.validate()
        .map_err(|_| anyhow::anyhow!("workspace_learning_goal_invalid"))?;
    ensure!(
        goal.schema == "oev2.workspace-goal/1"
            && anchor["project_id"].as_str() == Some(goal.project_id.as_str())
            && same_reference(&goal.project, project_ref)
            && same_reference(&goal.root_anchor, anchor_ref),
        "workspace_learning_root_changed"
    );
    ensure!(
        goal.acceptance_ids == anchor_acceptance(anchor)?,
        "workspace_learning_acceptance_changed"
    );
    Ok(())
}

/// Check the frozen goal artifact identity as well as its explicit root binding.
/// This supplements the old experience owner; it never changes old protocols.
pub fn validate_learning_binding(
    binding: &WorkspaceLearningBinding,
    goal: &WorkspaceGoal,
    goal_ref: &ArtifactReference,
    project_ref: &ArtifactReference,
    anchor_ref: &ArtifactReference,
    anchor: &Value,
) -> Result<()> {
    binding
        .validate()
        .map_err(|_| anyhow::anyhow!("workspace_learning_binding_invalid"))?;
    validate_learning_goal(goal, project_ref, anchor_ref, anchor)?;
    ensure!(
        binding.schema == "oev2.workspace-learning-binding/1"
            && binding.project_id == goal.project_id
            && same_reference(&binding.project, project_ref)
            && same_reference(&binding.root_anchor, anchor_ref)
            && same_reference(&binding.goal, goal_ref)
            && binding.acceptance_ids == goal.acceptance_ids,
        "workspace_learning_binding_changed"
    );
    Ok(())
}
