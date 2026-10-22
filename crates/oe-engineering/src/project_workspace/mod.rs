//! Durable project continuation. Source identity remains owned by the installation;
//! this module owns only project records and a conditional current commit pointer.
mod history;
pub mod receipts;
pub mod semantics;

use crate::{artifacts, current_task_view, harness, reviewed_reconciliation};
use anyhow::{Context, Result, bail, ensure};
use oe_contracts::{
    ArtifactReference, CurrentTaskViewInput, WorkspaceCommit, WorkspaceCommitRequest,
    WorkspaceGoal, WorkspaceHead, WorkspaceLearningBinding, WorkspaceManifest,
};
use oe_core::{
    AdvisoryFileLock, SkillRoot, read_regular_bytes, sha256, write_new, write_new_bytes,
};
use oe_kernel::project::{self, Project, route};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

const LIMIT: usize = 16 * 1024 * 1024;
const TOTAL_LIMIT: usize = 64 * 1024 * 1024;

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
fn load_commit(root: &SkillRoot, h: &WorkspaceHead) -> Result<WorkspaceCommit> {
    let c: WorkspaceCommit = serde_json::from_value(artifacts::json(root, &h.commit)?)?;
    dto(c.validate())?;
    dto(c.proposal.validate())?;
    ensure!(
        c.sequence == h.sequence && c.proposal.workspace_id == h.workspace_id,
        "workspace_commit_identity"
    );
    ensure!(
        h.commit.path == format!("commits/{}/commit.json", c.proposal.commit_id),
        "workspace_commit_location"
    );
    let mut cursor = c.clone();
    let mut seen = BTreeSet::from([h.commit.sha256.clone()]);
    let mut total = 0usize;
    for _ in 0..1024 {
        match &cursor.proposal.expected_parent {
            None => {
                ensure!(cursor.sequence == 1, "workspace_parent_sequence");
                history::validate_pending(root, h)?;
                return Ok(c);
            }
            Some(parent) => {
                ensure!(seen.insert(parent.sha256.clone()), "workspace_parent_cycle");
                let raw = artifacts::read(root, parent)?;
                total = total
                    .checked_add(raw.len())
                    .context("workspace_history_byte_overflow")?;
                ensure!(total <= TOTAL_LIMIT, "workspace_history_byte_limit");
                let before: WorkspaceCommit =
                    serde_json::from_value(oe_core::json::strict_json(&raw)?)?;
                dto(before.validate())?;
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

struct Checked {
    bytes: BTreeMap<String, Vec<u8>>,
    refs: BTreeMap<String, ArtifactReference>,
    total: usize,
}
impl Checked {
    fn new() -> Self {
        Self {
            bytes: BTreeMap::new(),
            refs: BTreeMap::new(),
            total: 0,
        }
    }
    fn read(&mut self, root: &SkillRoot, r: &ArtifactReference) -> Result<Vec<u8>> {
        if let Some(existing) = self.refs.get(&r.path) {
            ensure!(existing == r, "workspace_conflicting_reference");
            return Ok(self.bytes[&r.path].clone());
        }
        ensure!(self.refs.len() < 256, "workspace_source_count_limit");
        let raw = artifacts::read(root, r)?;
        self.total = self
            .total
            .checked_add(raw.len())
            .context("workspace_byte_overflow")?;
        ensure!(self.total <= TOTAL_LIMIT, "workspace_source_byte_limit");
        self.refs.insert(r.path.clone(), r.clone());
        self.bytes.insert(r.path.clone(), raw.clone());
        Ok(raw)
    }
    fn json(&mut self, root: &SkillRoot, r: &ArtifactReference) -> Result<Value> {
        Ok(oe_core::json::strict_json(&self.read(root, r)?)?)
    }
    fn recheck(&self, root: &SkillRoot) -> Result<()> {
        for (path, r) in &self.refs {
            ensure!(
                artifacts::read(root, r)? == self.bytes[path],
                "workspace_source_changed_during_operation"
            );
        }
        Ok(())
    }
    fn archive(&self, root: &SkillRoot) -> Result<()> {
        for (path, r) in &self.refs {
            let dest = root.private_path(Path::new(&format!("artifacts/{}", r.sha256)))?;
            if dest.try_exists()? {
                ensure!(
                    read_regular_bytes(&dest, LIMIT)? == self.bytes[path],
                    "workspace_archive_changed"
                );
            } else if let Err(error) = write_new_bytes(&dest, &self.bytes[path]) {
                // Another writer may have installed the identical content.
                ensure!(
                    read_regular_bytes(&dest, LIMIT).is_ok_and(|bytes| bytes == self.bytes[path]),
                    "workspace_archive_write: {error}"
                );
            }
        }
        Ok(())
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
fn semantic_inputs(
    root: &SkillRoot,
    q: &WorkspaceCommitRequest,
    checked: &mut Checked,
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
        ensure!(
            checked.json(root, &resolution.result)? == fresh,
            "workspace_review_result_not_replayed"
        );
        reviews.insert(resolution.result.path.clone(), fresh);
    }
    semantics::validate_semantics(
        &p,
        &q.project,
        &a,
        &q.requests,
        &q.corrections,
        &q.causal_claims,
        &checked.bytes,
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
            let verified = receipts::verify(root, stage, &p, &q.project)?;
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
    let m = manifest(root)?;
    ensure!(
        q.workspace_id == m.workspace_id,
        "workspace_identity_mismatch"
    );
    let initial_head = head(root, &m)?;
    let old = if let Some(parent) = &q.expected_parent {
        let raw: WorkspaceCommit = serde_json::from_value(artifacts::json(root, parent)?)?;
        Some(load_commit(
            root,
            &WorkspaceHead {
                schema: "oev2.workspace-head/1".into(),
                workspace_id: m.workspace_id.clone(),
                sequence: raw.sequence,
                commit: parent.clone(),
            },
        )?)
    } else {
        None
    };
    let mut checked = Checked::new();
    let (p, a, route, reviews) = semantic_inputs(root, q, &mut checked)?;
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
    // Keep a complete candidate on conflict. It never becomes current merely
    // because its files exist or its JSON says committed.
    let folder = root.new_private_dir(Path::new(&format!("commits/{}", q.commit_id)))?;
    let c = WorkspaceCommit {
        schema: "oev2.workspace-commit/1".into(),
        sequence: next_sequence,
        proposal: q.clone(),
        pending_decisions: pending.into_iter().collect(),
    };
    checked.archive(root)?;
    write_new(&folder.join("commit.json"), &json!(c))?;
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
    let c = load_commit(root, &h)?;
    let mut checked = Checked::new();
    let (p, a, route, _) = semantic_inputs(root, &c.proposal, &mut checked)?;
    ensure!(
        p.project_id == m.project_id,
        "workspace_project_identity_mismatch"
    );
    let input = CurrentTaskViewInput {
        schema: "oev2.current-task-view-input/1".into(),
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
    Ok(
        json!({"schema":"oev2.workspace-resume/1","status":status,"head":h,"project":c.proposal.project,"root_anchor":c.proposal.anchor,
        "root_requests":a["requests"],"root_object":a["root_object"],"deliverable":a["deliverable"],"acceptance":a["acceptance"],
        "corrections":c.proposal.corrections,"pending_decisions":c.pending_decisions,"stage":c.proposal.stage,"events":c.proposal.events,"causal_claims":c.proposal.causal_claims,
        "route":route,"task_view":view,"effects_require_readback":effect_unknown,"automatic_dispatch":false,"engineering_adoption":"not_granted"}),
    )
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
