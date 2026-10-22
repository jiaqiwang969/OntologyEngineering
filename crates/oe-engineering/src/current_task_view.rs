//! Read-only current task projection through the existing Project and kernel.
//! Candidate actions are unscheduled; this module grants no engineering adoption.
use anyhow::{Context, Result, ensure};
use oe_contracts::{
    ArtifactReference, CurrentTaskChange, CurrentTaskCheck, CurrentTaskCondition,
    CurrentTaskDecision, CurrentTaskGap, CurrentTaskSourceObservation, CurrentTaskView,
    CurrentTaskViewInput, CurrentTaskViewPort, FailureRecord, ObjectIdentity,
};
use oe_core::{SkillRoot, json::canonical_json, json::strict_json, sha256};
use oe_kernel::project::{self, Project};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

const SCHEMA: &str = "oev2.current-task-view/1";
const INPUT_SCHEMA: &str = "oev2.current-task-view-input/1";

pub mod method_use;

fn read(root: &SkillRoot, reference: &ArtifactReference) -> Result<Vec<u8>> {
    reference
        .validate()
        .map_err(|_| anyhow::anyhow!("current_task_reference_invalid"))?;
    crate::workflow::read_reference(
        root,
        &crate::workflow::Reference {
            path: reference.path.clone().into(),
            sha256: reference.sha256.clone(),
        },
    )
}
fn load_project(root: &SkillRoot, reference: &ArtifactReference) -> Result<(Project, Vec<u8>)> {
    let bytes = read(root, reference)?;
    let project: Project = serde_json::from_value(strict_json(&bytes)?)?;
    project::validate(&project)?;
    Ok((project, bytes))
}
fn encoded(value: &impl serde::Serialize) -> Result<String> {
    Ok(String::from_utf8(canonical_json(&serde_json::to_value(
        value,
    )?)?)?)
}
fn ids(value: &Value) -> Result<BTreeSet<String>> {
    value
        .as_array()
        .context("current_task_owner_affected_invalid")?
        .iter()
        .map(|v| {
            Ok(v.as_str()
                .context("current_task_owner_identity_invalid")?
                .to_owned())
        })
        .collect()
}
fn change(
    kind: &str,
    identity: &str,
    before: Option<String>,
    after: Option<String>,
) -> Option<CurrentTaskChange> {
    if before == after {
        return None;
    }
    let status = if before.is_none() {
        "added"
    } else if after.is_none() {
        "deleted"
    } else {
        "modified"
    };
    Some(CurrentTaskChange {
        kind: kind.into(),
        identity: identity.into(),
        previous_value: before,
        current_value: after,
        status: status.into(),
    })
}
fn mapping_changes(
    kind: &str,
    old: &BTreeMap<String, String>,
    current: &BTreeMap<String, String>,
    result: &mut Vec<CurrentTaskChange>,
) {
    let names: BTreeSet<_> = old.keys().chain(current.keys()).collect();
    for name in names {
        if let Some(row) = change(
            kind,
            name,
            old.get(name).cloned(),
            current.get(name).cloned(),
        ) {
            result.push(row);
        }
    }
}
fn observations(
    root: &SkillRoot,
    current: &Project,
    previous: Option<&Project>,
    refs: &[ArtifactReference],
) -> Result<Vec<CurrentTaskSourceObservation>> {
    ensure!(refs.len() <= 256, "current_task_source_inventory_limit");
    let mut paths = BTreeSet::new();
    let mut result = Vec::new();
    for reference in refs {
        reference
            .validate()
            .map_err(|_| anyhow::anyhow!("current_task_source_reference_invalid"))?;
        ensure!(
            paths.insert(&reference.path),
            "current_task_duplicate_source_path"
        );
        ensure!(
            current
                .evidence
                .values()
                .chain(previous.into_iter().flat_map(|p| p.evidence.values()))
                .any(|v| v == &reference.sha256),
            "current_task_source_not_project_evidence"
        );
        let evidence_ids = current
            .evidence
            .iter()
            .filter(|(_, v)| *v == &reference.sha256)
            .map(|(id, _)| id.clone())
            .collect();
        let attempt = (|| -> Result<Vec<u8>> {
            let path = root.expand(Path::new(&reference.path))?;
            ensure!(
                path.starts_with(root.path()),
                "engineering_reference_outside_root"
            );
            oe_core::read_regular_bytes(&path, oe_kernel::MAX_BYTES)
        })();
        let (actual_sha256, status, reason) = match attempt {
            Ok(bytes) => {
                let actual = sha256(bytes);
                if actual == reference.sha256 {
                    (Some(actual), "matched", None)
                } else {
                    (
                        Some(actual),
                        "drift",
                        Some("current_task_source_bytes_changed".into()),
                    )
                }
            }
            Err(error) => (None, "unavailable", Some(error.to_string())),
        };
        result.push(CurrentTaskSourceObservation {
            reference: reference.clone(),
            actual_sha256,
            status: status.into(),
            reason,
            evidence_ids,
        });
    }
    result.sort_by(|a, b| a.reference.path.cmp(&b.reference.path));
    Ok(result)
}
fn gap(
    id: String,
    origin: &str,
    state: &str,
    statement: String,
    needed: Vec<String>,
) -> CurrentTaskGap {
    CurrentTaskGap {
        gap_id: id,
        origin: origin.into(),
        state: state.into(),
        statement,
        needed_evidence: needed,
        required_conditions: Vec::new(),
        next_action:
            "candidate: obtain the listed evidence and repeat the existing bound review before use"
                .into(),
        arrangement: "unscheduled".into(),
    }
}

pub fn derive(root: &SkillRoot, input: &CurrentTaskViewInput) -> Result<CurrentTaskView> {
    input
        .validate()
        .map_err(|_| anyhow::anyhow!("current_task_input_invalid"))?;
    ensure!(input.schema == INPUT_SCHEMA, "current_task_input_schema");
    let (current, current_raw) = load_project(root, &input.current)?;
    let previous = if let Some(reference) = &input.previous {
        Some(load_project(root, reference)?)
    } else {
        None
    };
    let prior = previous.as_ref().map(|(p, _)| p);
    if let Some(prior) = prior {
        ensure!(
            prior.project_id == current.project_id,
            "project_identity_cannot_change"
        );
    }
    let work = if let Some(reference) = &input.work {
        let bytes = read(root, reference)?;
        let work: crate::workflow::WorkInput = serde_json::from_value(strict_json(&bytes)?)?;
        ensure!(
            root.expand(&work.project.path)? == root.expand(Path::new(&input.current.path))?
                && work.project.sha256 == input.current.sha256,
            "current_task_work_project_reference_changed"
        );
        crate::workflow::verify_project_binding(&current, &work)?;
        Some((work, bytes))
    } else {
        None
    };
    let source_observations = observations(root, &current, prior, &input.sources)?;
    let reconciliation = project::reconcile(prior.unwrap_or(&current), &current)?;
    let mut affected = ids(&reconciliation["affected"])?;
    let mut source_seeds = BTreeSet::new();
    for source in &source_observations {
        if source.status != "matched" {
            source_seeds.insert(source.reference.sha256.clone());
            source_seeds.extend(source.evidence_ids.iter().cloned());
            if let Some(prior) = prior {
                source_seeds.extend(
                    prior
                        .evidence
                        .iter()
                        .filter(|(_, s)| *s == &source.reference.sha256)
                        .map(|(id, _)| id.clone()),
                );
            }
        }
    }
    for p in prior.into_iter().chain(std::iter::once(&current)) {
        affected.extend(ids(&project::impact(p, &source_seeds)?["affected"])?);
    }
    let mut conditions = Vec::new();
    let scopes: BTreeSet<_> = current
        .conditions
        .keys()
        .chain(prior.into_iter().flat_map(|p| p.conditions.keys()))
        .collect();
    for scope in scopes {
        let before = prior.and_then(|p| p.conditions.get(scope)).cloned();
        let after = current.conditions.get(scope).cloned();
        let changed = if prior.is_none() {
            "unknown_prior"
        } else if before == after {
            "unchanged"
        } else if before.is_none() {
            "added"
        } else if after.is_none() {
            "deleted"
        } else {
            "modified"
        };
        conditions.push(CurrentTaskCondition {
            condition_scope: scope.clone(),
            previous_value: before,
            current_value: after,
            change: changed.into(),
        });
    }
    let mut changes = Vec::new();
    if let Some(prior) = prior {
        if let Some(row) = change(
            "purpose",
            "project:purpose",
            Some(prior.purpose.clone()),
            Some(current.purpose.clone()),
        ) {
            changes.push(row);
        }
        mapping_changes("object", &prior.objects, &current.objects, &mut changes);
        mapping_changes(
            "condition",
            &prior.conditions,
            &current.conditions,
            &mut changes,
        );
        mapping_changes("evidence", &prior.evidence, &current.evidence, &mut changes);
        let old = prior
            .decisions
            .iter()
            .map(|d| Ok((d.id.clone(), encoded(d)?)))
            .collect::<Result<BTreeMap<_, _>>>()?;
        let new = current
            .decisions
            .iter()
            .map(|d| Ok((d.id.clone(), encoded(d)?)))
            .collect::<Result<BTreeMap<_, _>>>()?;
        mapping_changes("decision", &old, &new, &mut changes);
        let old = prior
            .dependencies
            .iter()
            .map(|(k, v)| Ok((k.clone(), encoded(v)?)))
            .collect::<Result<BTreeMap<_, _>>>()?;
        let new = current
            .dependencies
            .iter()
            .map(|(k, v)| Ok((k.clone(), encoded(v)?)))
            .collect::<Result<BTreeMap<_, _>>>()?;
        mapping_changes("dependency", &old, &new, &mut changes);
    }
    changes.sort_by(|a, b| (&a.kind, &a.identity).cmp(&(&b.kind, &b.identity)));
    let old: BTreeMap<_, _> = prior
        .into_iter()
        .flat_map(|p| p.decisions.iter())
        .map(|d| (&d.id, d))
        .collect();
    let now: BTreeMap<_, _> = current.decisions.iter().map(|d| (&d.id, d)).collect();
    let names: BTreeSet<_> = old.keys().chain(now.keys()).copied().collect();
    let mut decisions = Vec::new();
    let mut gaps = Vec::new();
    let mut objects = BTreeSet::new();
    for name in names {
        let before = old.get(name).copied();
        let after = now.get(name).copied();
        let mut downstream = BTreeSet::new();
        let seed = BTreeSet::from([name.clone()]);
        for p in prior.into_iter().chain(std::iter::once(&current)) {
            downstream.extend(ids(&project::impact(p, &seed)?["affected"])?);
        }
        downstream.remove(name);
        let mut needs_review = affected.contains(name);
        if let Some(d) = after {
            needs_review |= current.objects.get(&d.object_id) != Some(&d.object_revision)
                || !current.conditions.contains_key(&d.condition_scope)
                || !current.evidence.values().any(|v| v == &d.source_sha256);
            objects.insert((
                d.object_id.clone(),
                d.object_revision.clone(),
                d.condition_scope.clone(),
                d.source_sha256.clone(),
            ));
        }
        let status = match after {
            None => "deleted",
            Some(_) if needs_review => "requires_review",
            Some(d) if d.conclusion == "unknown" => "unknown",
            Some(d) if d.conclusion == "refuted" => "refuted",
            Some(_) => "unchanged_declared",
        };
        if status != "unchanged_declared" {
            gaps.push(gap(format!("decision:{name}"),"decision",if status=="deleted" {"requires_review"}else {status},format!("Decision {name}: {status}; original declaration/history retained; no fresh physical acceptance inferred"),vec!["Existing current-condition, source and necessary downstream checks; independent engineering evidence before adoption".into()]));
        }
        decisions.push(CurrentTaskDecision {
            decision_id: name.clone(),
            prior_value: before.map(encoded).transpose()?,
            current_value: after.map(encoded).transpose()?,
            status: status.into(),
            downstream: downstream.into_iter().collect(),
        });
    }
    let mut requirements = vec![current.purpose.clone()];
    let mut necessary_functions = Vec::new();
    let mut physical_mechanisms = Vec::new();
    if let Some((work, _)) = &work {
        requirements.extend(work.acceptance.iter().cloned());
        necessary_functions = work.necessary_functions.clone();
        physical_mechanisms = work.physical_mechanisms.clone();
        for g in &work.gaps {
            let mut row = gap(
                g.gap_id.clone(),
                "work",
                "unknown",
                g.question.clone(),
                g.needed_evidence.clone(),
            );
            row.required_conditions = g.required_conditions.clone();
            gaps.push(row);
        }
        objects.insert((
            work.object.object_id.clone(),
            work.object.revision.clone(),
            work.object.condition_scope.clone(),
            work.object.source_sha256.clone(),
        ));
    }
    for (id, digest) in &current.evidence {
        if !source_observations
            .iter()
            .any(|s| s.status == "matched" && s.reference.sha256 == *digest)
        {
            gaps.push(gap(format!("source:{id}"),"source","unknown",format!("Current source {id} raw bytes are unavailable or drifted; registry SHA {digest} is a declaration"),vec![format!("Read original raw bytes for source {id} and compare to the registered SHA; applicability remains independent")]));
        }
    }
    let mut checks = Vec::new();
    for (object_id, revision, condition_scope, source_sha256) in objects {
        let object = ObjectIdentity {
            object_id: object_id.clone(),
            revision,
            condition_scope: condition_scope.clone(),
            source_sha256,
        };
        let mut results = oe_kernel::engineering::checks_current(root, &current, &object);
        if let Some((work, _)) = &work {
            if work.object == object {
                results.push(project::check_current_context(
                    &current,
                    &object,
                    work.project_context.as_ref(),
                ));
            }
        }
        for check in results {
            if check.execution != "completed" || check.conclusion != "pass" {
                gaps.push(gap(
                    format!(
                        "check:{object_id}:{condition_scope}:{}:{}",
                        object.source_sha256, check.id
                    ),
                    "check",
                    if check.execution == "failed" {
                        "failed"
                    } else {
                        "unknown"
                    },
                    format!(
                        "{}: execution {}, conclusion {}; necessary existing review remains open",
                        check.id, check.execution, check.conclusion
                    ),
                    check.failures.clone(),
                ));
            }
            checks.push(CurrentTaskCheck {
                object: object.clone(),
                check_id: check.id,
                execution: check.execution,
                conclusion: check.conclusion,
                coverage: check.coverage,
                failures: check.failures,
                result: check.result,
            });
        }
    }
    gaps.sort_by(|a, b| (&a.origin, &a.gap_id).cmp(&(&b.origin, &b.gap_id)));
    checks.sort_by(|a, b| {
        (
            &a.object.object_id,
            &a.object.condition_scope,
            &a.object.revision,
            &a.object.source_sha256,
            &a.check_id,
        )
            .cmp(&(
                &b.object.object_id,
                &b.object.condition_scope,
                &b.object.revision,
                &b.object.source_sha256,
                &b.check_id,
            ))
    });
    let view=CurrentTaskView {
        schema:SCHEMA.into(),input:input.clone(),input_sha256:sha256(canonical_json(&serde_json::to_value(input)?)?),
        project_id:current.project_id.clone(),project_revision:current.revision.clone(),project_raw_sha256:sha256(&current_raw),project_value_sha256:sha256(canonical_json(&serde_json::to_value(&current)?)?),
        requirements,necessary_functions,physical_mechanisms,objects:current.objects.clone(),conditions,changes,decisions,gaps,checks,source_observations,affected:affected.into_iter().collect(),
        unsupported:vec!["Project/1 has no task schedule, approval, execution timestamp or complete formal review history; every proposed action is candidate/unscheduled".into(),"Plain condition text is preserved as source data; no automatic extraction of authoritative new engineering facts or requirements; requirements shown are current purpose and explicitly bound WorkInput acceptance".into(),"Unsupplied source bodies, formal review coverage, physical evidence, cost and full engineering adoption remain unknown; a supported Project declaration is not fresh acceptance".into()],
        model_execution:"not_run".into(),engineering_adoption:"not_established".into(),project_update:"none".into(),arrangement:"unscheduled".into(),
    };
    view.validate()
        .map_err(|_| anyhow::anyhow!("current_task_output_invalid"))?;
    ensure!(
        read(root, &input.current)? == current_raw,
        "current_task_project_changed_during_projection"
    );
    if let (Some(reference), Some((_, raw))) = (&input.previous, &previous) {
        ensure!(
            read(root, reference)? == *raw,
            "current_task_previous_changed_during_projection"
        );
    }
    if let (Some(reference), Some((_, raw))) = (&input.work, &work) {
        ensure!(
            read(root, reference)? == *raw,
            "current_task_work_changed_during_projection"
        );
    }
    ensure!(
        observations(root, &current, prior, &input.sources)? == view.source_observations,
        "current_task_source_changed_during_projection"
    );
    Ok(view)
}

/// A whole prior view can be stale even when refreshed unrelated decisions stay.
pub fn verify_view(root: &SkillRoot, view: &CurrentTaskView) -> Result<()> {
    view.validate()
        .map_err(|_| anyhow::anyhow!("current_task_view_invalid"))?;
    ensure!(view.schema == SCHEMA, "current_task_view_schema");
    ensure!(
        view.source_observations
            .iter()
            .all(|s| s.status == "matched"),
        "current_task_source_freshness_unknown"
    );
    ensure!(
        derive(root, &view.input)? == *view,
        "current_task_view_stale"
    );
    Ok(())
}

pub struct CurrentTaskViewExecutor<'a> {
    root: &'a SkillRoot,
}
impl<'a> CurrentTaskViewExecutor<'a> {
    pub fn new(root: &'a SkillRoot) -> Self {
        Self { root }
    }
}
impl CurrentTaskViewPort for CurrentTaskViewExecutor<'_> {
    fn derive(
        &self,
        input: &CurrentTaskViewInput,
    ) -> std::result::Result<CurrentTaskView, FailureRecord> {
        derive(self.root, input)
            .map_err(|error| crate::artifacts::failure("current_task_view", error))
    }
}
