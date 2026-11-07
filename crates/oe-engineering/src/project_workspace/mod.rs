//! Durable project continuation. Source identity remains owned by the installation;
//! this module owns only project records and a conditional current commit pointer.
mod history;
pub mod receipts;
pub mod semantics;

use crate::source_cache::Material;
use crate::{artifacts, current_task_view, harness, reviewed_reconciliation};
use anyhow::{bail, ensure, Context, Result};
use oe_contracts::{
    ArtifactReference, CurrentTaskViewInput, WorkspaceCommit, WorkspaceCommitRequest,
    WorkspaceGoal, WorkspaceHead, WorkspaceLearningBinding, WorkspaceManifest,
    WorkspaceResolution,
};
use oe_core::{
    read_regular_bytes, sha256, write_new, write_new_bytes, AdvisoryFileLock, SkillRoot,
};
use oe_kernel::project::{self, route, Project};
use semantics::SourceMaterial;
use serde_json::{json, Value};
use std::borrow::Cow;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

const LIMIT: usize = crate::source_cache::FILE_LIMIT;
const TOTAL_LIMIT: usize = crate::source_cache::CACHE_LIMIT;

fn dto<T>(r: std::result::Result<T, Vec<String>>) -> Result<T> {
    r.map_err(|e| anyhow::anyhow!("workspace_contract: {}", e.join(",")))
}
fn id(s: &str) -> Result<()> {
    ensure!(
        !s.is_empty()
            && s.len() <= 128
            && s.bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'),
        "workspace_invalid_id"
    );
    Ok(())
}
fn explicit(root: &SkillRoot) -> Result<()> {
    ensure!(
        root.data_path() != root.path(),
        "workspace_explicit_project_home_required"
    );
    Ok(())
}
fn read_json(root: &SkillRoot, path: &str) -> Result<Value> {
    Ok(oe_core::json::strict_json(&read_regular_bytes(
        &root.private_path(Path::new(path))?,
        LIMIT,
    )?)?)
}
pub fn initialize(root: &SkillRoot, workspace_id: &str, project_id: &str) -> Result<Value> {
    explicit(root)?;
    id(workspace_id)?;
    ensure!(
        !project_id.trim().is_empty(),
        "workspace_project_id_required"
    );
    let manifest = WorkspaceManifest {
        schema: "oev2.workspace-manifest/1".into(),
        workspace_id: workspace_id.into(),
        project_id: project_id.into(),
    };
    dto(manifest.validate())?;
    let _lock = AdvisoryFileLock::acquire(&root.private_path(Path::new("workspace.lock"))?, true)?;
    write_new(
        &root.private_path(Path::new("manifest.json"))?,
        &json!(manifest),
    )?;
    for name in ["commits", "artifacts", "runs", "views"] {
        oe_core::ensure_directory(&root.private_path(Path::new(name))?)?;
    }
    Ok(
        json!({"schema":"oev2.workspace-init-result/1", "manifest":manifest,"status":"initialized","engineering_adoption":"not_granted"}),
    )
}
fn manifest(root: &SkillRoot) -> Result<WorkspaceManifest> {
    explicit(root)?;
    let m: WorkspaceManifest = serde_json::from_value(read_json(root, "manifest.json")?)?;
    dto(m.validate())?;
    id(&m.workspace_id)?;
    Ok(m)
}
fn head(root: &SkillRoot, m: &WorkspaceManifest) -> Result<Option<WorkspaceHead>> {
    let path = root.private_path(Path::new("HEAD.json"))?;
    if !path.try_exists()? {
        return Ok(None);
    }
    let h: WorkspaceHead = serde_json::from_value(read_json(root, "HEAD.json")?)?;
    dto(h.validate())?;
    ensure!(
        h.workspace_id == m.workspace_id && h.sequence > 0,
        "workspace_head_identity"
    );
    Ok(Some(h))
}
/// Load a commit and validate its complete history, under the given budget or
/// else the head's own request profile.
fn load_commit_within(
    root: &SkillRoot,
    h: &WorkspaceHead,
    budget: Option<history::HistoryBudget>,
) -> Result<(WorkspaceCommit, history::HistoryUse)> {
    let c: WorkspaceCommit = serde_json::from_value(artifacts::json(root, &h.commit)?)?;
    dto(c.validate())?;
    dto(c.proposal.validate())?;
    check_commit_schema(&c)?;
    ensure!(
        c.sequence == h.sequence && c.proposal.workspace_id == h.workspace_id,
        "workspace_commit_identity"
    );
    ensure!(
        h.commit.path == format!("commits/{}/commit.json", c.proposal.commit_id),
        "workspace_commit_location"
    );
    let budget = match budget {
        Some(budget) => budget,
        None => history::history_budget(&c.proposal.schema)?,
    };
    let mut cursor = c.clone();
    let mut seen = BTreeSet::from([h.commit.sha256.clone()]);
    let mut total = 0usize;
    for _ in 0..history::HISTORY_LIMIT {
        match &cursor.proposal.expected_parent {
            None => {
                ensure!(cursor.sequence == 1, "workspace_parent_sequence");
                let used = history::validate_pending(root, h, budget)?;
                return Ok((c, used));
            }
            Some(parent) => {
                ensure!(seen.insert(parent.sha256.clone()), "workspace_parent_cycle");
                let raw = artifacts::read(root, parent)?;
                total = total
                    .checked_add(raw.len())
                    .context("workspace_history_byte_overflow")?;
                ensure!(total <= budget.commit_bytes, "workspace_history_byte_limit");
                let before: WorkspaceCommit =
                    serde_json::from_value(oe_core::json::strict_json(&raw)?)?;
                dto(before.validate())?;
                dto(before.proposal.validate())?;
                check_commit_schema(&before)?;
                ensure!(
                    before.sequence.checked_add(1) == Some(cursor.sequence)
                        && before.proposal.workspace_id == h.workspace_id
                        && parent.path
                            == format!("commits/{}/commit.json", before.proposal.commit_id),
                    "workspace_parent_sequence"
                );
                cursor = before;
            }
        }
    }
    bail!("workspace_history_count_limit")
}
fn archived(root: &SkillRoot, r: &ArtifactReference) -> Result<Vec<u8>> {
    let bytes = read_regular_bytes(
        &root.private_path(Path::new(&format!("artifacts/{}", r.sha256)))?,
        LIMIT,
    )?;
    ensure!(sha256(&bytes) == r.sha256, "workspace_archive_changed");
    Ok(bytes)
}

/// Explicit resource profiles; older requests never inherit the larger budget.
fn source_limit(schema: &str) -> Result<usize> {
    match schema {
        "oev2.workspace-commit-request/1" => Ok(256),
        "oev2.workspace-commit-request/2" => Ok(1024),
        "oev2.workspace-commit-request/3" => Ok(1024),
        "oev2.workspace-commit-request/4" => Ok(crate::source_cache::EXTENDED_PATH_LIMIT),
        "oev2.workspace-commit-request/5" => Ok(crate::source_cache::LARGE_EXTENDED_PATH_LIMIT),
        "oev2.workspace-commit-request/6" => Ok(crate::source_cache::LARGE_EXTENDED_PATH_LIMIT),
        _ => bail!("workspace_request_schema"),
    }
}
fn commit_schema(schema: &str) -> Result<&'static str> {
    match schema {
        "oev2.workspace-commit-request/1" => Ok("oev2.workspace-commit/1"),
        "oev2.workspace-commit-request/2" => Ok("oev2.workspace-commit/2"),
        "oev2.workspace-commit-request/3" => Ok("oev2.workspace-commit/3"),
        "oev2.workspace-commit-request/4" => Ok("oev2.workspace-commit/4"),
        "oev2.workspace-commit-request/5" => Ok("oev2.workspace-commit/5"),
        "oev2.workspace-commit-request/6" => Ok("oev2.workspace-commit/6"),
        _ => bail!("workspace_request_schema"),
    }
}
fn check_commit_schema(commit: &WorkspaceCommit) -> Result<()> {
    ensure!(
        commit.schema == commit_schema(&commit.proposal.schema)?,
        "workspace_commit_schema_mismatch"
    );
    Ok(())
}

struct Checked {
    bytes: BTreeMap<String, Material>,
    refs: BTreeMap<String, ArtifactReference>,
    total: usize,
    stage_reasoning: Option<Value>,
    retained: usize,
    compress: bool,
    source_limit: usize,
    payload_limit: usize,
}
impl Checked {
    fn for_request(q: &WorkspaceCommitRequest) -> Result<Self> {
        Ok(Self {
            bytes: BTreeMap::new(),
            refs: BTreeMap::new(),
            total: 0,
            stage_reasoning: None,
            retained: 0,
            compress: matches!(
                q.schema.as_str(),
                "oev2.workspace-commit-request/3"
                    | "oev2.workspace-commit-request/4"
                    | "oev2.workspace-commit-request/5"
                    | "oev2.workspace-commit-request/6"
            ),
            payload_limit: match q.schema.as_str() {
                "oev2.workspace-commit-request/5" | "oev2.workspace-commit-request/6" => {
                    crate::source_cache::LARGE_EXTENDED_CACHE_LIMIT
                }
                "oev2.workspace-commit-request/4" => crate::source_cache::EXTENDED_CACHE_LIMIT,
                _ => TOTAL_LIMIT,
            },
            source_limit: source_limit(&q.schema)?,
        })
    }
    fn read(&mut self, root: &SkillRoot, r: &ArtifactReference) -> Result<Vec<u8>> {
        if let Some(existing) = self.refs.get(&r.path) {
            ensure!(existing == r, "workspace_conflicting_reference");
            return Ok(self.source_bytes(r)?.into_owned());
        }
        ensure!(
            self.refs.len() < self.source_limit,
            "workspace_source_count_limit"
        );
        let raw = artifacts::read(root, r)?;
        self.total = self
            .total
            .checked_add(raw.len())
            .context("workspace_byte_overflow")?;
        let material = Material::encode(&raw, self.compress)?;
        self.retained = self
            .retained
            .checked_add(material.retained_len())
            .context("workspace_byte_overflow")?;
        ensure!(
            self.retained <= self.payload_limit,
            "workspace_source_byte_limit"
        );
        self.refs.insert(r.path.clone(), r.clone());
        self.bytes.insert(r.path.clone(), material);
        Ok(raw)
    }
    fn json(&mut self, root: &SkillRoot, r: &ArtifactReference) -> Result<Value> {
        Ok(oe_core::json::strict_json(&self.read(root, r)?)?)
    }
    fn recheck(&self, root: &SkillRoot) -> Result<()> {
        for r in self.refs.values() {
            ensure!(
                artifacts::read(root, r)? == self.source_bytes(r)?.as_ref(),
                "workspace_source_changed_during_operation"
            );
        }
        Ok(())
    }
    fn archive(&self, root: &SkillRoot) -> Result<()> {
        for r in self.refs.values() {
            let raw = self.source_bytes(r)?;
            let dest = root.private_path(Path::new(&format!("artifacts/{}", r.sha256)))?;
            if dest.try_exists()? {
                ensure!(
                    read_regular_bytes(&dest, LIMIT)? == raw.as_ref(),
                    "workspace_archive_changed"
                );
            } else if let Err(error) = write_new_bytes(&dest, raw.as_ref()) {
                // Another writer may have installed the identical content.
                ensure!(
                    read_regular_bytes(&dest, LIMIT).is_ok_and(|bytes| bytes == raw.as_ref()),
                    "workspace_archive_write: {error}"
                );
            }
        }
        Ok(())
    }
}
impl SourceMaterial for Checked {
    fn source_bytes(&self, reference: &ArtifactReference) -> Result<Cow<'_, [u8]>> {
        ensure!(
            self.refs.get(&reference.path) == Some(reference),
            "workspace_conflicting_reference"
        );
        let raw = self
            .bytes
            .get(&reference.path)
            .context("workspace_source_material_missing")?
            .decode()?;
        ensure!(
            sha256(raw.as_ref()) == reference.sha256,
            "workspace_source_material_changed"
        );
        Ok(raw)
    }
}
fn route_report(
    p: &Project,
    a: &Value,
    p_ref: &ArtifactReference,
    a_ref: &ArtifactReference,
) -> Result<Value> {
    if a["schema"] == route::ANCHOR2_SCHEMA {
        Ok(json!(route::audit2(
            p,
            &serde_json::from_value(a.clone())?,
            &p_ref.sha256,
            &a_ref.sha256
        )?))
    } else {
        Ok(json!(route::audit(
            p,
            &serde_json::from_value(a.clone())?,
            &p_ref.sha256,
            &a_ref.sha256
        )?))
    }
}
/// A review already in the chain and recorded by another program keeps the identity it ran under:
/// history replay has verified the record, and a rebuild or upgrade reads it without re-signing it.
/// The program that recorded a review still re-executes it, and a new review is always executed.
fn recorded_by_another_program(
    chain: Option<&WorkspaceCommitRequest>,
    resolution: &WorkspaceResolution,
    saved: &Value,
) -> Result<bool> {
    let in_chain = chain.is_some_and(|c| {
        c.reviews
            .iter()
            .chain(c.corrections.iter().flat_map(|o| o.resolutions.iter()))
            .any(|r| r == resolution)
    });
    if !in_chain {
        return Ok(false);
    }
    let running = oe_kernel::executable_digest()?;
    Ok(saved["reviews"].as_array().is_some_and(|rows| {
        rows.iter().any(|row| {
            row["receipt"]["executable_sha256"]
                .as_str()
                .is_some_and(|recorded| recorded != running)
        })
    }))
}

fn semantic_inputs(
    root: &SkillRoot,
    q: &WorkspaceCommitRequest,
    checked: &mut Checked,
    chain: Option<&WorkspaceCommitRequest>,
) -> Result<(Project, Value, Value, BTreeMap<String, Value>)> {
    dto(q.validate())?;
    id(&q.commit_id)?;
    let p: Project = serde_json::from_value(checked.json(root, &q.project)?)?;
    project::validate(&p)?;
    let a = checked.json(root, &q.anchor)?;
    let route = route_report(&p, &a, &q.project, &q.anchor)?;
    ensure!(route["status"] != "invalid", "workspace_invalid_route");
    for r in &q.sources {
        checked.read(root, r)?;
    }
    for r in &q.requests {
        checked.read(root, &r.source)?;
    }
    for c in &q.causal_claims {
        for r in &c.source_refs {
            checked.read(root, r)?;
        }
    }
    let mut reviews = BTreeMap::new();
    for resolution in q
        .reviews
        .iter()
        .chain(q.corrections.iter().flat_map(|o| o.resolutions.iter()))
    {
        let request = checked.json(root, &resolution.request)?;
        for key in ["before", "current"] {
            checked.read(root, &serde_json::from_value(request[key].clone())?)?;
        }
        for source in request["sources"]
            .as_array()
            .context("workspace_review_sources_missing")?
        {
            checked.read(root, &serde_json::from_value(source.clone())?)?;
        }
        for review in request["reviews"]
            .as_array()
            .context("workspace_review_requests_missing")?
        {
            for key in ["binding", "evidence", "receipt"] {
                checked.read(root, &serde_json::from_value(review[key].clone())?)?;
            }
        }
        checked.read(root, &resolution.result)?;
        let saved = checked.json(root, &resolution.result)?;
        if recorded_by_another_program(chain, resolution, &saved)? {
            reviews.insert(resolution.result.path.clone(), saved);
            continue;
        }
        let fresh = match request["schema"].as_str() {
            Some("oev2.reviewed-reconciliation-request/1") => {
                json!(reviewed_reconciliation::reconcile_reviewed(
                    root,
                    &serde_json::from_value(request)?
                )?)
            }
            Some("oev2.reviewed-reconciliation-request/2") => {
                json!(reviewed_reconciliation::reconcile_reviewed2(
                    root,
                    &serde_json::from_value(request)?
                )?)
            }
            _ => bail!("workspace_resolution_owner_required"),
        };
        ensure!(saved == fresh, "workspace_review_result_not_replayed");
        reviews.insert(resolution.result.path.clone(), fresh);
    }
    semantics::validate_semantics(
        &p,
        &q.project,
        &a,
        &q.requests,
        &q.corrections,
        &q.causal_claims,
        checked,
        &reviews,
    )?;
    for resolution in &q.reviews {
        ensure!(
            checked.json(root, &resolution.result)? == reviews[&resolution.result.path],
            "workspace_review_result_not_replayed"
        );
        ensure!(
            reviews[&resolution.result.path]["current"] == json!(q.project),
            "workspace_review_not_current"
        );
    }
    for obligation in &q.corrections {
        if obligation.status == "resolved" {
            continue;
        }
        let request = a["requests"]
            .as_array()
            .unwrap()
            .iter()
            .find(|u| u["id"] == obligation.request_id)
            .context("workspace_correction_request_missing")?;
        for affected in request["affects"]
            .as_array()
            .context("workspace_correction_acceptance")?
        {
            ensure!(
                !a["acceptance"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|item| item["id"] == *affected && item["status"] == "closed"),
                "workspace_unresolved_correction_closes_acceptance"
            );
        }
    }
    let acceptance: BTreeSet<_> = a["acceptance"]
        .as_array()
        .context("workspace_acceptance_missing")?
        .iter()
        .filter_map(|x| x["id"].as_str())
        .collect();
    let mut events = BTreeSet::new();
    for e in &q.events {
        dto(e.validate())?;
        ensure!(
            !e.sources.is_empty() && !events.contains(&e.id),
            "workspace_event_source_or_identity"
        );
        ensure!(
            e.corrects.iter().all(|x| events.contains(x)),
            "workspace_event_correction_missing"
        );
        ensure!(
            e.acceptance_ids
                .iter()
                .all(|x| acceptance.contains(x.as_str())),
            "workspace_event_acceptance_unknown"
        );
        for r in &e.sources {
            checked.read(root, r)?;
        }
        events.insert(e.id.clone());
    }
    if let Some(work) = &q.work {
        checked.read(root, work)?;
    }
    if let Some(checkpoint) = &q.checkpoint {
        checked.read(root, checkpoint)?;
        let loaded = harness::load_step(root, &json!(checkpoint))?;
        // The original owner verifies its complete attachments. Its recorded
        // status still does not qualify the current Project for adoption.
        ensure!(
            loaded["status"] == "recorded" && loaded["state"]["project_id"] == p.project_id,
            "workspace_checkpoint_not_recorded"
        );
        let parent = Path::new(&checkpoint.path)
            .parent()
            .context("workspace_checkpoint_parent")?;
        let mut current = None;
        for (name, digest) in loaded["artifacts"]
            .as_object()
            .context("workspace_checkpoint_artifacts")?
        {
            oe_semantic::safe_relative(name)?;
            let reference = ArtifactReference {
                path: parent
                    .join(name)
                    .to_str()
                    .context("workspace_checkpoint_path")?
                    .into(),
                sha256: digest
                    .as_str()
                    .context("workspace_checkpoint_digest")?
                    .into(),
            };
            let raw = checked.read(root, &reference)?;
            if name == "input.json" {
                current =
                    Some(oe_core::json::strict_json(&raw)?["harness"]["current_project"].clone());
            }
        }
        ensure!(
            current == Some(json!(q.project)),
            "workspace_checkpoint_current_project_mismatch"
        );
    }
    if let Some(stage) = &q.stage {
        dto(stage.validate())?;
        ensure!(
            !stage.acceptance_ids.is_empty()
                && stage
                    .acceptance_ids
                    .iter()
                    .all(|x| acceptance.contains(x.as_str())),
            "workspace_stage_acceptance_unknown"
        );
        let route_stage = a["stages"]
            .as_array()
            .context("workspace_stages_missing")?
            .iter()
            .find(|s| s["id"] == stage.id)
            .context("workspace_stage_not_in_route")?;
        ensure!(
            stage.acceptance_ids.iter().all(|x| route_stage["advances"]
                .as_array()
                .is_some_and(|v| v.contains(&json!(x)))),
            "workspace_stage_route_mismatch"
        );
        if let Some(r) = &stage.receipt {
            checked.read(root, r)?;
        }
        ensure!(
            stage.execution != "results_recorded" || stage.receipt.is_some(),
            "workspace_stage_receipt_required"
        );
        if stage.execution == "results_recorded" {
            let verified = receipts::verify_bound(root, stage, &p, &q.project, &a)?;
            checked.stage_reasoning = verified["native"].get("reasoning").cloned();
            for reference in verified["references"]
                .as_array()
                .context("workspace_receipt_sources_missing")?
            {
                checked.read(root, &serde_json::from_value(reference.clone())?)?;
            }
        }
    }
    checked.recheck(root)?;
    Ok((p, a, route, reviews))
}
fn transition(
    root: &SkillRoot,
    old: Option<&WorkspaceCommit>,
    q: &WorkspaceCommitRequest,
    a: &Value,
) -> Result<()> {
    if let Some(old) = old {
        let old_a = oe_core::json::strict_json(&archived(root, &old.proposal.anchor)?)?;
        let before = old_a["requests"]
            .as_array()
            .context("workspace_old_requests")?;
        let after = a["requests"].as_array().context("workspace_new_requests")?;
        ensure!(
            after.starts_with(before),
            "workspace_request_history_rewritten"
        );
        let old_p: Project = serde_json::from_value(oe_core::json::strict_json(&archived(
            root,
            &old.proposal.project,
        )?)?)?;
        let current_p: Project = serde_json::from_value(artifacts::json(root, &q.project)?)?;
        let old_stages = old_a["stages"].as_array().context("workspace_old_stages")?;
        let new_stages = a["stages"].as_array().context("workspace_new_stages")?;
        ensure!(
            new_stages.len() >= old_stages.len()
                && old_stages.iter().zip(new_stages).all(|(before, after)| [
                    "id",
                    "object_id",
                    "relation",
                    "relation_target",
                    "advances"
                ]
                .iter()
                .all(|key| before[*key] == after[*key])),
            "workspace_stage_history_rewritten"
        );
        let revised_goal = old_p.purpose != current_p.purpose
            || old_a["root_object"] != a["root_object"]
            || old_a["deliverable"] != a["deliverable"]
            || old_a["acceptance"].as_array().unwrap().iter().any(|prev| {
                !a["acceptance"].as_array().unwrap().iter().any(|now| {
                    now["id"] == prev["id"]
                        && now["statement"] == prev["statement"]
                        && now["functions"] == prev["functions"]
                        && prev["decisions"].as_array().is_none_or(|ds| {
                            ds.iter().all(|d| {
                                now["decisions"].as_array().is_some_and(|ns| ns.contains(d))
                            })
                        })
                })
            });
        ensure!(
            !revised_goal
                || after[before.len()..]
                    .iter()
                    .any(|u| u["kind"] == "correction" || u["kind"] == "approval"),
            "workspace_goal_change_needs_source_request"
        );
        ensure!(
            q.events.starts_with(&old.proposal.events),
            "workspace_event_history_rewritten"
        );
        ensure!(
            old.proposal.corrections.iter().all(|c| q
                .corrections
                .iter()
                .any(|n| n.id == c.id && n.request_id == c.request_id)),
            "workspace_correction_obligation_dropped"
        );
        for previous in &old.proposal.corrections {
            let current = q.corrections.iter().find(|c| c.id == previous.id).unwrap();
            ensure!(
                previous
                    .affected_decisions
                    .iter()
                    .all(|d| current.affected_decisions.contains(d)),
                "workspace_correction_impact_shrunk"
            );
        }
        if let Some(stage) = &q.stage {
            if old.proposal.stage.as_ref().map(|s| &s.id) != Some(&stage.id) {
                ensure!(
                    !old_a["stages"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|s| s["id"] == stage.id),
                    "workspace_stage_identity_reused"
                );
            }
        }
    }
    let previous = old.and_then(|c| c.proposal.stage.as_ref());
    match (previous, q.stage.as_ref()) {
        (None, Some(s)) => ensure!(
            s.execution == "planned" && s.receipt.is_none(),
            "workspace_stage_must_start_planned"
        ),
        (Some(a), Some(b)) if a.id == b.id => {
            ensure!(
                a.acceptance_ids == b.acceptance_ids && a.intent == b.intent,
                "workspace_stage_intent_changed"
            );
            let allowed = a == b
                || matches!(
                    (a.execution.as_str(), b.execution.as_str()),
                    ("planned", "dispatched" | "cancelled")
                        | ("dispatched", "effect_unknown" | "results_recorded")
                        | ("effect_unknown", "results_recorded")
                );
            ensure!(allowed, "workspace_stage_transition_invalid");
        }
        (Some(a), b) => {
            ensure!(
                a.execution == "results_recorded" || a.execution == "cancelled",
                "workspace_unfinished_action_cannot_disappear"
            );
            if let Some(b) = b {
                ensure!(
                    b.execution == "planned" && b.receipt.is_none(),
                    "workspace_stage_must_start_planned"
                );
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn commit(root: &SkillRoot, q: &WorkspaceCommitRequest) -> Result<Value> {
    commit_within(root, q, None)
}
fn commit_within(
    root: &SkillRoot,
    q: &WorkspaceCommitRequest,
    budget: Option<history::HistoryBudget>,
) -> Result<Value> {
    let m = manifest(root)?;
    ensure!(
        q.workspace_id == m.workspace_id,
        "workspace_identity_mismatch"
    );
    let initial_head = head(root, &m)?;
    // The new request's profile bounds the complete history it carries, the
    // parent chain included; a smaller profile cannot carry a larger history.
    let budget = match budget {
        Some(budget) => budget,
        None => history::history_budget(&q.schema)?,
    };
    let (old, carried) = if let Some(parent) = &q.expected_parent {
        let raw: WorkspaceCommit = serde_json::from_value(artifacts::json(root, parent)?)?;
        let (old, used) = load_commit_within(
            root,
            &WorkspaceHead {
                schema: "oev2.workspace-head/1".into(),
                workspace_id: m.workspace_id.clone(),
                sequence: raw.sequence,
                commit: parent.clone(),
            },
            Some(budget),
        )?;
        (Some(old), used.commit_bytes)
    } else {
        (None, 0)
    };
    let mut checked = Checked::for_request(q)?;
    let (p, a, route, reviews) =
        semantic_inputs(root, q, &mut checked, old.as_ref().map(|c| &c.proposal))?;
    ensure!(
        p.project_id == m.project_id,
        "workspace_project_identity_mismatch"
    );
    transition(root, old.as_ref(), q, &a)?;
    let starts = q.stage.as_ref().is_some_and(|stage| {
        old.as_ref().and_then(|c| c.proposal.stage.as_ref()) != Some(stage)
            && matches!(stage.execution.as_str(), "planned" | "dispatched")
    });
    if starts {
        ensure!(
            route["status"] == "anchored",
            "workspace_stage_route_not_anchored"
        );
        if q.stage
            .as_ref()
            .is_some_and(|s| s.execution == "dispatched")
        {
            ensure!(
                p.evidence
                    .values()
                    .all(|hash| q.sources.iter().any(|r| &r.sha256 == hash)),
                "workspace_stage_sources_incomplete"
            );
        }
    }
    let mut pending: BTreeSet<String> = old
        .as_ref()
        .map(|c| c.pending_decisions.iter().cloned().collect())
        .unwrap_or_default();
    if let Some(before) = &old {
        let prior: Project = serde_json::from_value(oe_core::json::strict_json(&archived(
            root,
            &before.proposal.project,
        )?)?)?;
        let impact = project::reconcile(&prior, &p)?;
        for d in impact["decisions"]
            .as_array()
            .context("workspace_impact_decisions")?
        {
            pending.insert(
                d["decision_id"]
                    .as_str()
                    .context("workspace_impact_identity")?
                    .into(),
            );
        }
    }
    for review in reviews
        .values()
        .filter(|r| r["current"] == json!(q.project))
    {
        if let Some(satisfied) = review["review_satisfied"].as_array() {
            for d in satisfied {
                if let Some(d) = d.as_str() {
                    pending.remove(d);
                }
            }
        }
        if let Some(required) = review["still_requires_review"].as_array() {
            for d in required {
                if let Some(d) = d.as_str() {
                    pending.insert(d.into());
                }
            }
        }
    }
    history::propagate_pending(&p, &mut pending)?;
    for item in a["acceptance"].as_array().unwrap() {
        if item["status"] == "closed" {
            ensure!(
                !item["decisions"].as_array().is_some_and(|ds| ds
                    .iter()
                    .any(|d| d.as_str().is_some_and(|id| pending.contains(id)))),
                "workspace_stale_decision_closes_acceptance"
            );
        }
    }
    let next_sequence = old.as_ref().map_or(Ok(1), |c| {
        c.sequence
            .checked_add(1)
            .context("workspace_sequence_overflow")
    })?;
    let c = WorkspaceCommit {
        schema: commit_schema(&q.schema)?.into(),
        sequence: next_sequence,
        proposal: q.clone(),
        pending_decisions: pending.into_iter().collect(),
    };
    // The next read counts this commit too: refuse before writing anything when
    // the new head could not be read back within its own profile.
    let commit_value = json!(c);
    let commit_bytes = serde_json::to_vec_pretty(&commit_value)?.len() + 1;
    ensure!(
        usize::try_from(next_sequence).is_ok_and(|n| n <= budget.commits),
        "workspace_history_chain_budget_or_cycle"
    );
    ensure!(
        carried
            .checked_add(commit_bytes)
            .is_some_and(|total| total <= budget.commit_bytes),
        "workspace_history_byte_budget"
    );
    // Keep a complete candidate on conflict. It never becomes current merely
    // because its files exist or its JSON says committed.
    let folder = root.new_private_dir(Path::new(&format!("commits/{}", q.commit_id)))?;
    checked.archive(root)?;
    write_new(&folder.join("commit.json"), &commit_value)?;
    let commit_ref = artifacts::reference(root, &folder.join("commit.json"))?;
    let lock = match AdvisoryFileLock::acquire(
        &root.private_path(Path::new("workspace.lock"))?,
        true,
    ) {
        Ok(lock) => lock,
        Err(error) if error.to_string() == "resource_busy" => {
            let result = json!({"schema":"oev2.workspace-commit-result/1","status":"conflict","reason":"resource_busy","candidate":commit_ref,"current":null,"next":"resume_and_reconcile_after_writer_finishes","engineering_adoption":"not_granted"});
            write_new(&folder.join("conflict.json"), &result)?;
            oe_core::sync_directory(&folder)?;
            oe_core::sync_directory(&root.data_path().join("commits"))?;
            return Ok(result);
        }
        Err(error) => return Err(error),
    };
    let current = head(root, &m)?;
    if current.as_ref().map(|h| &h.commit) != q.expected_parent.as_ref() || current != initial_head
    {
        let result = json!({"schema":"oev2.workspace-commit-result/1","status":"conflict","candidate":commit_ref,"current":current,"engineering_adoption":"not_granted"});
        write_new(&folder.join("conflict.json"), &result)?;
        return Ok(result);
    }
    checked.recheck(root)?;
    lock.ensure_current()?;
    ensure!(manifest(root)? == m, "workspace_manifest_changed");
    let next = WorkspaceHead {
        schema: "oev2.workspace-head/1".into(),
        workspace_id: m.workspace_id,
        sequence: next_sequence,
        commit: commit_ref,
    };
    let prepared = folder.join("HEAD.prepared.json");
    write_new(&prepared, &json!(next))?;
    for dir in [
        &folder,
        &root.data_path().join("commits"),
        &root.data_path().join("artifacts"),
        root.data_path(),
    ] {
        oe_core::sync_directory(dir)?;
    }
    oe_core::rename_regular_file(&prepared, &root.private_path(Path::new("HEAD.json"))?)?;
    Ok(
        json!({"schema":"oev2.workspace-commit-result/1","status":"committed","head":next,"route":route,"engineering_adoption":"not_granted"}),
    )
}

pub fn resume(root: &SkillRoot) -> Result<Value> {
    let m = manifest(root)?;
    let _lock = AdvisoryFileLock::acquire(&root.private_path(Path::new("workspace.lock"))?, false)?;
    let h = head(root, &m)?.context("workspace_no_committed_state")?;
    let (c, history_use) = load_commit_within(root, &h, None)?;
    let mut checked = Checked::for_request(&c.proposal)?;
    let (p, a, route, _) = semantic_inputs(root, &c.proposal, &mut checked, Some(&c.proposal))?;
    ensure!(
        p.project_id == m.project_id,
        "workspace_project_identity_mismatch"
    );
    let input = CurrentTaskViewInput {
        schema: match c.proposal.schema.as_str() {
            "oev2.workspace-commit-request/5" | "oev2.workspace-commit-request/6" => {
                "oev2.current-task-view-input/6"
            }
            "oev2.workspace-commit-request/4" => "oev2.current-task-view-input/5",
            "oev2.workspace-commit-request/3" => "oev2.current-task-view-input/4",
            "oev2.workspace-commit-request/2" => "oev2.current-task-view-input/3",
            _ => "oev2.current-task-view-input/1",
        }
        .into(),
        current: c.proposal.project.clone(),
        previous: None,
        work: c.proposal.work.clone(),
        sources: c.proposal.sources.clone(),
    };
    let view = current_task_view::derive(root, &input)?;
    let mut status = "ready";
    if view
        .source_observations
        .iter()
        .any(|x| x.status != "matched")
        || p.evidence
            .values()
            .any(|hash| !c.proposal.sources.iter().any(|r| &r.sha256 == hash))
    {
        status = "incomplete";
    }
    if route["status"] != "anchored" {
        status = "needs_review";
    }
    let effect_unknown = c
        .proposal
        .stage
        .as_ref()
        .is_some_and(|s| s.execution == "dispatched" || s.execution == "effect_unknown");
    if effect_unknown {
        status = "effect_unknown";
    }
    checked.recheck(root)?;
    ensure!(
        head(root, &m)?.as_ref() == Some(&h),
        "workspace_head_changed_during_resume"
    );
    let mut result = json!({"schema":"oev2.workspace-resume/1","status":status,"head":h,"project":c.proposal.project,"root_anchor":c.proposal.anchor,
        "root_requests":a["requests"],"root_object":a["root_object"],"deliverable":a["deliverable"],"acceptance":a["acceptance"],
        "corrections":c.proposal.corrections,"pending_decisions":c.pending_decisions,"stage":c.proposal.stage,"events":c.proposal.events,"causal_claims":c.proposal.causal_claims,
        "route":route,"task_view":view,"effects_require_readback":effect_unknown,"automatic_dispatch":false,"engineering_adoption":"not_granted"});
    if checked.compress {
        result["source_budget"] = json!({"profile":match c.proposal.schema.as_str() { "oev2.workspace-commit-request/5" | "oev2.workspace-commit-request/6" => "oev2.workspace-source-cache/3", "oev2.workspace-commit-request/4" => "oev2.workspace-source-cache/2", _ => "oev2.workspace-source-cache/1" },
            "paths":checked.refs.len(),"path_limit":checked.source_limit,
            "logical_bytes":checked.total,"retained_payload_bytes":checked.retained,
            "payload_limit_bytes":checked.payload_limit,"per_file_limit_bytes":LIMIT,
            "maximum_logical_bytes":checked.source_limit as u64 * LIMIT as u64,
            "scope":"Operation-local lossless payload, not total process memory; original files and archives unchanged"});
    }
    if c.proposal.schema == "oev2.workspace-commit-request/6" {
        let budget = history::history_budget(&c.proposal.schema)?;
        result["history_budget"] = json!({"profile":"oev2.workspace-history/2",
            "commits":history_use.commits,"commit_limit":budget.commits,
            "commit_bytes":history_use.commit_bytes,"commit_limit_bytes":budget.commit_bytes,
            "archive_bytes":history_use.archive_bytes,"archive_limit_bytes":budget.archive_bytes,
            "scope":"Complete history from the head is read on every resume and commit; finite headroom, not a process memory guarantee"});
    }
    if let Some(reasoning) = checked.stage_reasoning {
        result["reasoning"] = reasoning;
    }
    Ok(result)
}

pub fn render_resume(root: &SkillRoot, output: &Path) -> Result<Value> {
    let r = resume(root)?;
    let mut text = format!(
        "# Project continuation\n\nStatus: {}\n\nCurrent commit: {} / {}\n\n",
        r["status"], r["head"]["commit"]["path"], r["head"]["commit"]["sha256"]
    );
    for (label, key) in [
        ("Original requests and corrections", "root_requests"),
        ("Complete acceptance", "acceptance"),
        ("Correction obligations", "corrections"),
        ("Pending decision reviews", "pending_decisions"),
        ("Current stage", "stage"),
        ("Current gaps", "task_view"),
        ("Event history", "events"),
        ("Causal claims and unknowns", "causal_claims"),
    ] {
        text.push_str(&format!(
            "## {label}\n\n```json\n{}\n```\n\n",
            serde_json::to_string_pretty(&r[key])?
        ));
    }
    if let Some(reasoning) = r.get("reasoning") {
        text.push_str(&format!(
            "## Engineering relations and next work\n\n```json\n{}\n```\n\n",
            serde_json::to_string_pretty(reasoning)?
        ));
    }
    text.push_str("This is a derived view. Re-run workspace resume before continuing. Recorded results do not grant engineering adoption; dispatched actions require readback before retry.\n");
    ensure!(text.len() <= LIMIT, "workspace_resume_view_budget");
    write_new_bytes(&root.private_path(output)?, text.as_bytes())?;
    Ok(r)
}

pub fn begin(root: &SkillRoot, expected: &ArtifactReference, stage_id: &str) -> Result<Value> {
    let _lock = AdvisoryFileLock::acquire(&root.private_path(Path::new("workspace.lock"))?, false)?;
    let r = resume(root)?;
    ensure!(
        r["head"]["commit"] == json!(expected),
        "workspace_stale_resume"
    );
    ensure!(r["status"] == "ready", "workspace_not_ready_for_stage");
    ensure!(
        r["stage"]["id"] == stage_id && r["stage"]["execution"] == "planned",
        "workspace_stage_not_planned"
    );
    Ok(
        json!({"schema":"oev2.workspace-stage-check/1","status":"ready","head":r["head"],"stage":r["stage"],"corrections":r["corrections"],"engineering_adoption":"not_granted","automatic_dispatch":false}),
    )
}

pub fn learning_goal(root: &SkillRoot, output: &Path) -> Result<Value> {
    let _lock = AdvisoryFileLock::acquire(&root.private_path(Path::new("workspace.lock"))?, false)?;
    let r = resume(root)?;
    ensure!(r["status"] == "ready", "workspace_not_ready_for_learning");
    let m = manifest(root)?;
    let goal = WorkspaceGoal {
        schema: "oev2.workspace-goal/1".into(),
        project_id: m.project_id,
        project: serde_json::from_value(r["project"].clone())?,
        root_anchor: serde_json::from_value(r["root_anchor"].clone())?,
        acceptance_ids: r["acceptance"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["id"].as_str().unwrap().to_owned())
            .collect(),
    };
    let goal_path = root.private_path(output)?;
    oe_core::ensure_directory(
        goal_path
            .parent()
            .context("workspace_goal_parent_missing")?,
    )?;
    write_new(&goal_path, &json!(goal))?;
    Ok(
        json!({"goal":artifacts::reference(root,&root.private_path(output)?)?,"head":r["head"],"method_benefit":"not_measured"}),
    )
}

pub fn learning_freeze(root: &SkillRoot, input: &Value, output: &Path) -> Result<Value> {
    let _lock = AdvisoryFileLock::acquire(&root.private_path(Path::new("workspace.lock"))?, false)?;
    let r = resume(root)?;
    ensure!(r["status"] == "ready", "workspace_not_ready_for_learning");
    let goal_ref: ArtifactReference = serde_json::from_value(input["goal"].clone())?;
    let goal: WorkspaceGoal = serde_json::from_value(artifacts::json(root, &goal_ref)?)?;
    let project_ref = serde_json::from_value(r["project"].clone())?;
    let anchor_ref = serde_json::from_value(r["root_anchor"].clone())?;
    let anchor = artifacts::json(root, &anchor_ref)?;
    semantics::validate_learning_goal(&goal, &project_ref, &anchor_ref, &anchor)?;
    let project: Project = serde_json::from_value(artifacts::json(root, &project_ref)?)?;
    ensure!(
        input["candidate"]["project_id"] == project.project_id,
        "workspace_learning_project_mismatch"
    );
    let object = &input["candidate"]["object"];
    ensure!(
        object["object_id"].as_str().is_some_and(|id| project
            .objects
            .get(id)
            .is_some_and(|v| json!(v) == object["revision"]))
            && object["condition_scope"]
                .as_str()
                .is_some_and(|scope| project.conditions.contains_key(scope))
            && object["source_sha256"]
                .as_str()
                .is_some_and(|hash| project.evidence.values().any(|v| v == hash)),
        "workspace_learning_object_mismatch"
    );
    let frozen = oe_kernel::learning::freeze(root, input, output)?;
    let binding = WorkspaceLearningBinding {
        schema: "oev2.workspace-learning-binding/1".into(),
        project_id: goal.project_id.clone(),
        project: project_ref,
        root_anchor: anchor_ref,
        goal: goal_ref.clone(),
        acceptance_ids: goal.acceptance_ids.clone(),
    };
    let binding_path = root.private_path(output)?.join("workspace-binding.json");
    write_new(&binding_path, &json!(binding))?;
    let binding_ref = artifacts::reference(root, &binding_path)?;
    Ok(
        json!({"schema":"oev2.workspace-learning-freeze/1","head":r["head"],"goal":goal_ref,"binding":binding_ref,"result":frozen,"method_benefit":"not_measured"}),
    )
}

/// Recheck the complete root goal before continuing an existing method trial.
/// Native experience assess/decide still owns frozen-evaluator validation.
pub fn with_learning_candidate<T>(
    root: &SkillRoot,
    candidate: &Path,
    action: impl FnOnce() -> Result<T>,
) -> Result<T> {
    let Some(goal) = oe_kernel::learning::workspace_goal(root, candidate)? else {
        ensure!(
            root.data_path() == root.path()
                || !root
                    .private_path(Path::new("manifest.json"))?
                    .try_exists()?,
            "workspace_learning_goal_required"
        );
        return action();
    };
    let lock = AdvisoryFileLock::acquire(&root.private_path(Path::new("workspace.lock"))?, false)?;
    let restored = resume(root)?;
    ensure!(
        restored["status"] == "ready",
        "workspace_not_ready_for_learning"
    );
    let project_ref = serde_json::from_value(restored["project"].clone())?;
    let anchor_ref = serde_json::from_value(restored["root_anchor"].clone())?;
    semantics::validate_learning_goal(
        &goal,
        &project_ref,
        &anchor_ref,
        &artifacts::json(root, &anchor_ref)?,
    )?;
    let result = action()?;
    lock.ensure_current()?;
    Ok(result)
}

pub fn learning_check(root: &SkillRoot, reference: &ArtifactReference) -> Result<Value> {
    let _lock = AdvisoryFileLock::acquire(&root.private_path(Path::new("workspace.lock"))?, false)?;
    let restored = resume(root)?;
    ensure!(
        restored["status"] == "ready",
        "workspace_not_ready_for_learning"
    );
    let binding: WorkspaceLearningBinding =
        serde_json::from_value(artifacts::json(root, reference)?)?;
    let goal: WorkspaceGoal = serde_json::from_value(artifacts::json(root, &binding.goal)?)?;
    let project_ref = serde_json::from_value(restored["project"].clone())?;
    let anchor_ref = serde_json::from_value(restored["root_anchor"].clone())?;
    let anchor = artifacts::json(root, &anchor_ref)?;
    semantics::validate_learning_binding(
        &binding,
        &goal,
        &binding.goal,
        &project_ref,
        &anchor_ref,
        &anchor,
    )?;
    Ok(
        json!({"schema":"oev2.workspace-learning-check/1","status":"root_consistent","binding":reference,"head":restored["head"],"trial_validation":"requires_native_experience_owner","method_benefit":"not_measured","engineering_adoption":"not_granted"}),
    )
}

#[cfg(test)]
mod source_capacity_tests {
    use super::*;

    #[test]
    fn source_cache_exact_payload_boundaries_charge_distinct_paths() {
        let mut state = 0x4a17b9e3f251c68du64;
        let raw: Vec<u8> = (0..16 * 1024 * 1024)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state as u8
            })
            .collect();
        for (version, count) in [(3, 4), (4, 8), (5, 16)] {
            let install = tempfile::tempdir().unwrap();
            let home = tempfile::tempdir().unwrap();
            std::fs::write(
                install.path().join("SKILL.md"),
                b"resource boundary fixture",
            )
            .unwrap();
            // macOS /var is a symlink; resolve the owned temp roots before
            // passing them to the production no-symlink source guard.
            let install_path = std::fs::canonicalize(install.path()).unwrap();
            let home_path = std::fs::canonicalize(home.path()).unwrap();
            let root = SkillRoot::new(&install_path)
                .unwrap()
                .with_project_data_root(&home_path)
                .unwrap();
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples/project-workspace/fixtures/initial-commit.json");
            let mut q: WorkspaceCommitRequest =
                serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
            q.schema = format!("oev2.workspace-commit-request/{version}");
            let mut checked = Checked::for_request(&q).unwrap();
            let digest = sha256(&raw);
            for i in 0..count {
                let name = format!("payload-{i}.bin");
                std::fs::write(home.path().join(&name), &raw).unwrap();
                let reference = ArtifactReference {
                    path: name,
                    sha256: digest.clone(),
                };
                assert_eq!(checked.read(&root, &reference).unwrap(), raw);
                // Re-reading the same path is not a second retained source.
                assert_eq!(checked.read(&root, &reference).unwrap(), raw);
            }
            assert_eq!(checked.retained, count * 16 * 1024 * 1024);
            assert_eq!(checked.total, count * 16 * 1024 * 1024);
            std::fs::write(home.path().join("extra.bin"), b"x").unwrap();
            let error = checked
                .read(
                    &root,
                    &ArtifactReference {
                        path: "extra.bin".into(),
                        sha256: sha256(b"x"),
                    },
                )
                .unwrap_err();
            assert!(format!("{error:#}").contains("workspace_source_byte_limit"));
        }
    }
}

#[cfg(test)]
mod history_capacity_tests {
    use super::*;
    use history::HistoryBudget;

    const MIB: usize = 1024 * 1024;
    const WIDE: HistoryBudget = HistoryBudget {
        commits: 1024,
        commit_bytes: usize::MAX,
        archive_bytes: usize::MAX,
    };

    struct Home {
        _install: tempfile::TempDir,
        _store: tempfile::TempDir,
        path: std::path::PathBuf,
        root: SkillRoot,
    }

    fn home() -> Home {
        let install = tempfile::tempdir().unwrap();
        let store = tempfile::tempdir().unwrap();
        std::fs::write(install.path().join("SKILL.md"), b"history boundary fixture").unwrap();
        let install_path = std::fs::canonicalize(install.path()).unwrap();
        let path = std::fs::canonicalize(store.path()).unwrap();
        let fixtures =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/project-workspace/fixtures");
        for file in std::fs::read_dir(fixtures).unwrap() {
            let file = file.unwrap();
            std::fs::copy(file.path(), path.join(file.file_name())).unwrap();
        }
        let root = SkillRoot::new(&install_path)
            .unwrap()
            .with_project_data_root(&path)
            .unwrap();
        initialize(&root, "bracket-demo", "bracket-continuation-demo").unwrap();
        Home {
            _install: install,
            _store: store,
            path,
            root,
        }
    }

    fn request(
        home: &Home,
        id: &str,
        parent: Option<&ArtifactReference>,
    ) -> WorkspaceCommitRequest {
        let mut q: WorkspaceCommitRequest =
            serde_json::from_slice(&std::fs::read(home.path.join("initial-commit.json")).unwrap())
                .unwrap();
        q.schema = "oev2.workspace-commit-request/6".into();
        q.commit_id = id.into();
        q.expected_parent = parent.cloned();
        q
    }

    fn current(home: &Home) -> WorkspaceHead {
        serde_json::from_slice(&std::fs::read(home.path.join("HEAD.json")).unwrap()).unwrap()
    }

    fn bytes(home: &Home, reference: &ArtifactReference) -> usize {
        std::fs::metadata(home.path.join(&reference.path))
            .unwrap()
            .len() as usize
    }

    fn first(home: &Home) -> ArtifactReference {
        let committed =
            commit_within(&home.root, &request(home, "first", None), Some(WIDE)).unwrap();
        assert_eq!(committed["status"], "committed");
        serde_json::from_value(committed["head"]["commit"].clone()).unwrap()
    }

    fn refuses<T: std::fmt::Debug>(result: Result<T>, reason: &str) {
        let error = format!("{:#}", result.unwrap_err());
        assert!(error.contains(reason), "expected {reason}, got {error}");
    }

    #[test]
    fn profiles_keep_the_legacy_history_budget_and_name_the_larger_one() {
        for version in 1..=5 {
            let schema = format!("oev2.workspace-commit-request/{version}");
            assert_eq!(
                history::history_budget(&schema).unwrap(),
                HistoryBudget {
                    commits: 1024,
                    commit_bytes: 64 * MIB,
                    archive_bytes: 64 * MIB
                }
            );
        }
        assert_eq!(
            history::history_budget("oev2.workspace-commit-request/6").unwrap(),
            HistoryBudget {
                commits: 1024,
                commit_bytes: 256 * MIB,
                archive_bytes: 256 * MIB
            }
        );
        refuses(
            history::history_budget("oev2.workspace-commit-request/7"),
            "workspace_request_schema",
        );
    }

    #[test]
    fn a_history_read_counts_every_commit_from_the_head_at_the_exact_boundary() {
        let home = home();
        let parent = first(&home);
        commit_within(
            &home.root,
            &request(&home, "second", Some(&parent)),
            Some(WIDE),
        )
        .unwrap();
        let head = current(&home);
        let total = bytes(&home, &parent) + bytes(&home, &head.commit);
        let (_, used) = load_commit_within(&home.root, &head, Some(WIDE)).unwrap();
        assert_eq!((used.commits, used.commit_bytes), (2, total));
        assert!(used.archive_bytes > 0);
        let exact = HistoryBudget {
            commits: 2,
            commit_bytes: total,
            archive_bytes: used.archive_bytes,
        };
        assert_eq!(
            load_commit_within(&home.root, &head, Some(exact))
                .unwrap()
                .1,
            used
        );
        refuses(
            load_commit_within(
                &home.root,
                &head,
                Some(HistoryBudget {
                    commit_bytes: total - 1,
                    ..exact
                }),
            ),
            "workspace_history_byte_budget",
        );
        refuses(
            load_commit_within(
                &home.root,
                &head,
                Some(HistoryBudget {
                    archive_bytes: used.archive_bytes - 1,
                    ..exact
                }),
            ),
            "workspace_history_archive_budget",
        );
        refuses(
            load_commit_within(
                &home.root,
                &head,
                Some(HistoryBudget {
                    commits: 1,
                    ..exact
                }),
            ),
            "workspace_history_chain_budget_or_cycle",
        );
    }

    #[test]
    fn a_commit_that_its_own_head_could_not_read_back_is_refused_before_writing() {
        // Measure the exact two-commit history in one home, then replay the same
        // deterministic requests in another with the counted and smaller budgets.
        let measured = home();
        let parent = first(&measured);
        commit_within(
            &measured.root,
            &request(&measured, "second", Some(&parent)),
            Some(WIDE),
        )
        .unwrap();
        let total = bytes(&measured, &parent) + bytes(&measured, &current(&measured).commit);
        let home = home();
        assert_eq!(first(&home), parent);
        let before = current(&home);
        for (budget, reason) in [
            (
                HistoryBudget {
                    commit_bytes: total - 1,
                    ..WIDE
                },
                "workspace_history_byte_budget",
            ),
            (
                HistoryBudget { commits: 1, ..WIDE },
                "workspace_history_chain_budget_or_cycle",
            ),
        ] {
            refuses(
                commit_within(
                    &home.root,
                    &request(&home, "second", Some(&parent)),
                    Some(budget),
                ),
                reason,
            );
            assert_eq!(current(&home), before);
            assert!(!home.path.join("commits/second").exists());
        }
        let exact = HistoryBudget {
            commits: 2,
            commit_bytes: total,
            ..WIDE
        };
        let committed = commit_within(
            &home.root,
            &request(&home, "second", Some(&parent)),
            Some(exact),
        )
        .unwrap();
        assert_eq!(committed["status"], "committed");
        assert_eq!(
            bytes(&home, &parent) + bytes(&home, &current(&home).commit),
            total
        );
    }
}
