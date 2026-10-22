//! Current, scope-bound engineering adoption. Project declarations remain declarations.
use crate::artifacts;
use anyhow::{Context, Result, ensure};
use oe_contracts::{
    ArtifactReference, EngineeringAdoptionPort, EngineeringAdoptionRequest,
    EngineeringAdoptionResult, FailureRecord, ObjectIdentity, ProjectReviewContext, ReviewRequest,
};
use oe_core::{SkillRoot, sha256, write_new};
use oe_kernel::{Binding, port::RustSemanticPort, project::Project};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

pub const POLICY: &str = "var/state/engineering-adoption-authority.json";
pub mod promoted2;

// One original owner/path, with explicit versions. Each entry still performs
// its own strict field parsing; neither protocol is a fallback for the other.
fn read_authority_policy(root: &SkillRoot) -> Result<(Value, String)> {
    let (policy, digest) = artifacts::private_policy(root, POLICY)?;
    ensure!(
        matches!(
            policy["schema"].as_str(),
            Some("oev2.engineering-adoption-authority/1" | "oev2.engineering-adoption-authority/2")
        ),
        "adoption_authority_schema_unsupported"
    );
    Ok((policy, digest))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Authority {
    schema: String,
    project_id: String,
    decision_id: String,
    claim_scope: String,
    actor_scopes: BTreeMap<String, BTreeSet<String>>,
    approvers: BTreeSet<String>,
    approved_approvals: BTreeSet<String>,
    bundle: String,
    approved_package_sha256: String,
    required_checks: BTreeSet<String>,
    required_sources: BTreeMap<String, ArtifactReference>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Approval {
    schema: String,
    approver: String,
    decision_id: String,
    claim_scope: String,
    project_context: ProjectReviewContext,
    sources: BTreeMap<String, String>,
}

pub struct AdoptionExecutor<'a> {
    pub root: &'a SkillRoot,
    pub output: &'a Path,
}
impl EngineeringAdoptionPort for AdoptionExecutor<'_> {
    fn adopt(
        &self,
        input: &EngineeringAdoptionRequest,
    ) -> std::result::Result<EngineeringAdoptionResult, FailureRecord> {
        adopt(self.root, input, self.output)
            .map_err(|e| artifacts::failure("engineering_adoption", e))
    }
}

struct Prepared {
    project: Project,
    decision: usize,
    current: ProjectReviewContext,
    review: oe_kernel::Receipt,
    policy_sha256: String,
    approval: Approval,
}
fn prepare(root: &SkillRoot, input: &EngineeringAdoptionRequest) -> Result<Prepared> {
    input
        .validate()
        .map_err(|_| anyhow::anyhow!("adoption_request_invalid"))?;
    ensure!(
        input.schema == "oev2.engineering-adoption-request/1",
        "adoption_schema_unsupported"
    );
    let (policy, policy_sha256) = read_authority_policy(root)?;
    let authority: Authority = serde_json::from_value(policy)?;
    ensure!(
        authority.schema == "oev2.engineering-adoption-authority/1"
            && authority.decision_id == input.decision_id
            && authority.claim_scope == input.claim_scope,
        "adoption_authority_scope_mismatch"
    );
    ensure!(
        authority
            .actor_scopes
            .get(&input.actor)
            .is_some_and(|scopes| scopes.contains(&input.claim_scope)),
        "adoption_actor_not_authorized"
    );
    ensure!(
        !authority.required_checks.is_empty()
            && !authority.required_sources.is_empty()
            && !authority.approvers.is_empty()
            && !authority.approved_approvals.is_empty(),
        "adoption_authority_requirements_empty"
    );
    let project: Project = serde_json::from_value(artifacts::json(root, &input.project)?)?;
    oe_kernel::project::validate(&project)?;
    ensure!(
        project.project_id == authority.project_id,
        "adoption_project_scope_mismatch"
    );
    let binding: Binding = serde_json::from_value(artifacts::json(root, &input.binding)?)?;
    binding.validate()?;
    ensure!(
        input.claim_scope == binding.condition_scope,
        "adoption_claim_outside_review_scope"
    );
    ensure!(
        binding.project_id == project.project_id
            && binding.required_checks == authority.required_checks
            && input.bundle == authority.bundle,
        "adoption_required_review_changed"
    );
    // A private bundle name identifies a mutable location, not the approved rules.
    // Pin the actual validated archive in the separate trusted authority policy.
    let package = oe_semantic::load_bundle(root.path(), &input.bundle)?;
    ensure!(
        package.spec["sha256"] == authority.approved_package_sha256
            && binding.package_sha256 == authority.approved_package_sha256,
        "adoption_package_not_authority_bound"
    );
    let object = ObjectIdentity {
        object_id: binding.object_id.clone(),
        revision: binding.object_revision.clone(),
        condition_scope: binding.condition_scope.clone(),
        source_sha256: binding.source_sha256.clone(),
    };
    let current = oe_kernel::project::context(&project, &object)?;
    ensure!(
        binding.project_context.as_ref() == Some(&current),
        "adoption_current_context_changed"
    );
    let basis = oe_kernel::engineering::basis(&project, &object)?
        .context("adoption_requires_current_engineering_basis")?;
    ensure!(
        !basis.assertions.is_empty() && !basis.acceptance.is_empty(),
        "adoption_engineering_obligations_empty"
    );
    ensure!(
        oe_kernel::engineering::checks_current(root, &project, &object)
            .iter()
            .all(|c| authority.required_checks.contains(&c.id)),
        "adoption_engineering_checks_not_required"
    );
    ensure!(
        basis
            .facts
            .iter()
            .flat_map(|f| &f.source_refs)
            .chain(basis.assertions.iter().flat_map(|a| &a.source_refs))
            .all(|id| authority.required_sources.contains_key(id)),
        "adoption_engineering_sources_not_bound"
    );
    let decision = project
        .decisions
        .iter()
        .position(|d| d.id == input.decision_id)
        .context("adoption_decision_missing")?;
    let d = &project.decisions[decision];
    ensure!(
        d.object_id == object.object_id
            && d.object_revision == object.revision
            && d.condition_scope == object.condition_scope
            && d.source_sha256 == object.source_sha256,
        "adoption_decision_binding_mismatch"
    );
    ensure!(
        authority
            .approved_approvals
            .contains(&input.approval.sha256),
        "adoption_approval_not_authority_bound"
    );
    let approval: Approval = serde_json::from_value(artifacts::json(root, &input.approval)?)?;
    ensure!(
        approval.schema == "oev2.engineering-adoption-approval/1"
            && approval.decision_id == input.decision_id
            && approval.claim_scope == input.claim_scope
            && approval.project_context == current,
        "adoption_approval_not_current"
    );
    ensure!(
        approval.approver != input.actor && authority.approvers.contains(&approval.approver),
        "adoption_independent_approval_required"
    );
    let source_hashes: BTreeMap<_, _> = authority
        .required_sources
        .iter()
        .map(|(id, source)| (id.clone(), source.sha256.clone()))
        .collect();
    ensure!(
        approval.sources == source_hashes
            && authority
                .required_sources
                .iter()
                .all(|(id, source)| project.evidence.get(id) == Some(&source.sha256)),
        "adoption_source_obligations_missing"
    );
    for source in authority.required_sources.values() {
        artifacts::read(root, source)?;
    }
    let evidence = artifacts::read(root, &input.evidence)?;
    let request = ReviewRequest {
        object,
        evidence_sha256: sha256(&evidence),
        required_checks: binding.required_checks.iter().cloned().collect(),
    };
    let port = RustSemanticPort {
        root: root.path().to_owned(),
        binding,
        bundle: input.bundle.clone(),
        queries: input.query_ids.clone(),
        shapes: input.shape_ids.clone(),
        evidence: evidence.clone(),
    };
    let historical = port.execute(&request, None)?;
    let review = oe_kernel::project::review_current(root, &project, &evidence, historical)?;
    ensure!(
        review.execution == "completed"
            && review.conclusion == "pass"
            && review
                .checks
                .iter()
                .all(|c| c.execution == "completed" && c.conclusion == "pass"),
        "adoption_required_review_not_passed"
    );
    // Recheck all authority and source bytes before creating any completed adoption.
    ensure!(
        artifacts::private_policy(root, POLICY)?.1 == policy_sha256,
        "adoption_policy_changed"
    );
    for reference in [
        &input.project,
        &input.binding,
        &input.evidence,
        &input.approval,
    ] {
        artifacts::read(root, reference)?;
    }
    for source in authority.required_sources.values() {
        artifacts::read(root, source)?;
    }
    ensure!(
        oe_semantic::load_bundle(root.path(), &input.bundle)?.spec == package.spec,
        "adoption_package_changed_during_execution"
    );
    Ok(Prepared {
        project,
        decision,
        current,
        review,
        policy_sha256,
        approval,
    })
}

pub fn adopt(
    root: &SkillRoot,
    input: &EngineeringAdoptionRequest,
    output: &Path,
) -> Result<EngineeringAdoptionResult> {
    let Prepared {
        mut project,
        decision,
        current,
        review,
        policy_sha256,
        approval,
    } = prepare(root, input)?;
    let output = root.new_private_dir(output)?;
    write_new(&output.join("review.json"), &serde_json::to_value(&review)?)?;
    let review_ref = artifacts::reference(root, &output.join("review.json"))?;
    project.decisions[decision].conclusion = "supported".into();
    project.decisions[decision].adopted = true;
    project.decisions[decision].basis =
        vec![review_ref.sha256.clone(), input.approval.sha256.clone()];
    oe_kernel::project::validate(&project)?;
    write_new(
        &output.join("project.json"),
        &serde_json::to_value(&project)?,
    )?;
    let project_ref = artifacts::reference(root, &output.join("project.json"))?;
    let record = json!({"schema":"oev2.engineering-adoption-record/1","execution":"completed",
        "decision_id":input.decision_id,"claim_scope":input.claim_scope,"actor":input.actor,
        "approver":approval.approver,"request":input,"authority_sha256":policy_sha256,
        "project_context":current,"review":review_ref,"project":project_ref,
        "engine_id":oe_kernel::ENGINE_ID,"engine_version":oe_kernel::ENGINE_VERSION,
        "executable_sha256":oe_kernel::executable_digest()?,
        "adoption":"scope_bound","broader_adoption":"not_established",
        "authority_boundary":"locally owned private policy and independent scope approval; not organizational cryptographic authentication"});
    write_new(&output.join("adoption.json"), &record)?;
    Ok(EngineeringAdoptionResult {
        schema: "oev2.engineering-adoption-result/1".into(),
        execution: "completed".into(),
        decision_id: input.decision_id.clone(),
        claim_scope: input.claim_scope.clone(),
        project: project_ref,
        review: review_ref,
        record: artifacts::reference(root, &output.join("adoption.json"))?,
    })
}

/// Recompute the inputs behind an adoption; old records do not grant fresh use.
pub fn verify(root: &SkillRoot, record: &Value) -> Result<Value> {
    ensure!(
        record["schema"] == "oev2.engineering-adoption-record/1",
        "adoption_record_schema"
    );
    let request: EngineeringAdoptionRequest = serde_json::from_value(record["request"].clone())?;
    let prepared = prepare(root, &request)?;
    ensure!(
        prepared.policy_sha256 == record["authority_sha256"],
        "adoption_authority_changed"
    );
    ensure!(
        serde_json::to_value(&prepared.current)? == record["project_context"]
            && record["decision_id"] == request.decision_id
            && record["claim_scope"] == request.claim_scope
            && record["actor"] == request.actor
            && record["approver"] == prepared.approval.approver,
        "adoption_record_context_changed"
    );
    ensure!(
        record["executable_sha256"] == oe_kernel::executable_digest()?,
        "adoption_executor_changed_requires_revalidation"
    );
    let review_ref = serde_json::from_value(record["review"].clone())?;
    let saved: oe_kernel::Receipt = serde_json::from_value(artifacts::json(root, &review_ref)?)?;
    ensure!(
        saved.execution == "completed"
            && saved.conclusion == "pass"
            && saved
                .checks
                .iter()
                .all(|c| c.execution == "completed" && c.conclusion == "pass")
            && serde_json::to_value(&saved.binding)?
                == serde_json::to_value(&prepared.review.binding)?
            && saved.evidence_sha256 == prepared.review.evidence_sha256
            && saved.coverage == prepared.review.coverage,
        "adoption_revalidation_failed"
    );
    let output_ref = serde_json::from_value(record["project"].clone())?;
    let output: Project = serde_json::from_value(artifacts::json(root, &output_ref)?)?;
    let mut expected = prepared.project;
    expected.decisions[prepared.decision].conclusion = "supported".into();
    expected.decisions[prepared.decision].adopted = true;
    expected.decisions[prepared.decision].basis =
        vec![review_ref.sha256, request.approval.sha256.clone()];
    ensure!(
        serde_json::to_value(&output)? == serde_json::to_value(&expected)?,
        "adoption_output_not_bound"
    );
    Ok(
        json!({"status":"current_scope_bound","decision_id":request.decision_id,
        "claim_scope":request.claim_scope,"broader_adoption":"not_established"}),
    )
}
