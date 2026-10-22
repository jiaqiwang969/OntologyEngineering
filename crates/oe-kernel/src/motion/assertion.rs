//! Explicit bounded native observations, separate from analytic mechanism_motion.
use crate::{
    CheckResult, engineering,
    project::{self, Project},
};
use anyhow::{Context, Result, ensure};
use oe_contracts::{
    CadMotionReceiveResult1, CadNativeMotionAssertion1, EngineeringAssertion, ObjectIdentity,
};
use oe_core::SkillRoot;
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn declaration(
    project: &Project,
    object: &ObjectIdentity,
    a: &EngineeringAssertion,
) -> Result<CadNativeMotionAssertion1> {
    a.validate()
        .map_err(|_| anyhow::anyhow!("native_motion_declaration_invalid"))?;
    let basis =
        engineering::basis(project, object)?.context("native_motion_current_basis_missing")?;
    let exact = serde_json::to_value(a)?;
    ensure!(
        basis
            .assertions
            .iter()
            .any(|v| serde_json::to_value(v).ok().as_ref() == Some(&exact)),
        "native_motion_assertion_not_in_current_project"
    );
    let i: CadNativeMotionAssertion1 = serde_json::from_value(a.requirements.clone())?;
    i.validate()
        .map_err(|_| anyhow::anyhow!("native_motion_assertion_invalid"))?;
    i.object
        .validate()
        .map_err(|_| anyhow::anyhow!("native_motion_object_invalid"))?;
    i.receive_request
        .validate()
        .map_err(|_| anyhow::anyhow!("native_motion_request_invalid"))?;
    ensure!(
        a.kind == "cad_native_motion" && i.object == *object && a.subject == object.object_id,
        "native_motion_current_identity_differs"
    );
    project::context(project, object)?;
    ensure!(
        a.source_refs.contains(&i.request_ref)
            && project.evidence.get(&i.request_ref) == Some(&i.receive_request.sha256),
        "native_motion_request_source_unbound"
    );
    ensure!(
        !a.affected_decisions.is_empty(),
        "native_motion_affected_decisions_missing"
    );
    for id in &a.source_refs {
        ensure!(
            project.evidence.contains_key(id),
            "native_motion_source_unbound"
        );
        let impact = project::impact(project, &BTreeSet::from([id.clone()]))?;
        let affected = impact["affected"]
            .as_array()
            .context("native_motion_impact_missing")?;
        ensure!(
            a.affected_decisions
                .iter()
                .all(|d| project.decisions.iter().any(|v| &v.id == d)
                    && affected.contains(&json!(d))),
            "native_motion_dependency_missing"
        );
    }
    Ok(i)
}

// append-only-project/1: retain all previous engineering inputs and outputs.
// Only the selected condition may append native assertions. Other fields,
// including goal_support and necessary acceptance, are compared in full.
fn applicable(previous: &Project, current: &Project, object: &ObjectIdentity) -> Result<()> {
    project::context(previous, object)?;
    project::context(current, object)?;
    ensure!(
        previous.schema == current.schema
            && previous.project_id == current.project_id
            && previous.purpose == current.purpose
            && previous.objects == current.objects,
        "native_motion_project_inputs_changed"
    );
    ensure!(
        previous.conditions.len() == current.conditions.len(),
        "native_motion_conditions_changed"
    );
    for (key, old) in &previous.conditions {
        let new = current
            .conditions
            .get(key)
            .context("native_motion_condition_removed")?;
        if old == new {
            continue;
        }
        ensure!(
            key == &object.condition_scope,
            "native_motion_other_condition_changed"
        );
        let old_basis =
            engineering::basis(previous, object)?.context("native_motion_old_basis_missing")?;
        let new_basis =
            engineering::basis(current, object)?.context("native_motion_current_basis_missing")?;
        let mut before = old_basis.original_value().clone();
        let mut after = new_basis.original_value().clone();
        let old_assertions = before
            .as_object_mut()
            .context("native_motion_old_basis_invalid")?
            .remove("assertions")
            .context("native_motion_old_assertions_missing")?;
        let new_assertions = after
            .as_object_mut()
            .context("native_motion_current_basis_invalid")?
            .remove("assertions")
            .context("native_motion_new_assertions_missing")?;
        ensure!(
            before == after,
            "native_motion_engineering_conditions_changed"
        );
        let old_rows = old_assertions
            .as_array()
            .context("native_motion_old_assertions_invalid")?;
        let new_rows = new_assertions
            .as_array()
            .context("native_motion_new_assertions_invalid")?;
        ensure!(
            new_rows.starts_with(old_rows),
            "native_motion_old_assertions_changed"
        );
        for row in &new_basis.assertions[old_rows.len()..] {
            declaration(current, object, row)?;
        }
    }
    ensure!(
        previous
            .evidence
            .iter()
            .all(|(k, v)| current.evidence.get(k) == Some(v)),
        "native_motion_old_source_changed"
    );
    ensure!(
        previous
            .dependencies
            .iter()
            .all(|(k, v)| current.dependencies.get(k) == Some(v)),
        "native_motion_old_dependency_changed"
    );
    ensure!(
        current.decisions.len() >= previous.decisions.len(),
        "native_motion_old_decision_removed"
    );
    for (old, new) in previous.decisions.iter().zip(&current.decisions) {
        ensure!(
            serde_json::to_value(old)? == serde_json::to_value(new)?,
            "native_motion_old_decision_changed"
        );
    }
    Ok(())
}

fn result(
    project: &Project,
    a: &EngineeringAssertion,
    conclusion: &str,
    reason: Option<String>,
    detail: Value,
    actual: bool,
) -> CheckResult {
    let mut r = engineering::result(
        &format!("engineering:{}", a.assertion_id),
        conclusion,
        reason,
        detail,
    );
    // A claim outcome is not a change of the source or the entire object.
    // Source/object changes still use Project::reconcile and its full graph.
    let mut seeds: BTreeSet<_> = a.affected_decisions.iter().cloned().collect();
    seeds.insert(a.assertion_id.clone());
    let mut affected: BTreeSet<_> = a.affected_decisions.iter().cloned().collect();
    if let Ok(impact) = project::impact(project, &seeds) {
        for row in impact["decisions"].as_array().into_iter().flatten() {
            if let Some(id) = row["decision_id"].as_str() {
                affected.insert(id.to_owned());
            }
        }
    }
    r.result["details"]["affected_decisions"] = json!(affected);
    r.result["details"]["original_sources_recomputed"] = json!(actual);
    r.coverage = vec!["original native motion/reopen request, declared samples and current append-only-project/1 applicability; no continuous motion, manufacturing or physical accuracy acceptance".into()];
    r
}

pub fn declared(
    project: &Project,
    object: &ObjectIdentity,
    a: &EngineeringAssertion,
) -> CheckResult {
    let reason = declaration(project, object, a)
        .err()
        .map(|e| e.to_string())
        .unwrap_or_else(|| "native_motion_original_sources_not_recomputed".into());
    result(
        project,
        a,
        "unknown",
        Some(reason),
        json!({"claim":a.requirements["claim"]}),
        false,
    )
}

fn assess(claim: &str, report: &CadMotionReceiveResult1) -> (&'static str, Option<&'static str>) {
    let complete = report.outcome == "accepted_bounded" && report.transport_state == "received";
    let discrete = complete
        && report.stream_state == "received"
        && report.motion_state == "received"
        && report.pair_identity_state == "received"
        && report.query_state == "received"
        && report.locally_checked_samples > 0
        && report.locally_checked_pairs > 0
        && report
            .interference_pairs
            .checked_add(report.no_interference_pairs)
            .and_then(|v| v.checked_add(report.touching_pairs))
            == Some(report.locally_checked_pairs);
    match claim {
        "discrete_pair_checks" if discrete => ("pass", None),
        "sampled_no_interference" if discrete => {
            if report.interference_pairs > 0 || report.touching_pairs > 0 {
                (
                    "violated",
                    Some("native_motion_sampled_interference_or_touching"),
                )
            } else {
                ("pass", None)
            }
        }
        "independent_reopen_bounded"
            if complete
                && report.save_state == "received"
                && report.reopen_state == "received"
                && report
                    .received_scope
                    .iter()
                    .any(|s| s == "independent_reopen_bounded_positions_and_geometry") =>
        {
            ("pass", None)
        }
        "discrete_pair_checks" | "sampled_no_interference" | "independent_reopen_bounded" => (
            "unknown",
            Some("native_motion_required_observation_incomplete"),
        ),
        _ => (
            "unknown",
            Some("native_motion_claim_outside_received_scope"),
        ),
    }
}

pub fn current(
    root: &SkillRoot,
    project: &Project,
    object: &ObjectIdentity,
    a: &EngineeringAssertion,
) -> CheckResult {
    let mut historical_observation = None;
    let checked = (|| -> Result<_> {
        let i = declaration(project, object, a)?;
        let raw = super::read(root, &i.receive_request)?;
        let v = oe_core::json::strict_json(&raw)?;
        let reopen = match v["schema"].as_str() {
            Some("oev2.cad-motion-receive/1") => false,
            Some("oev2.cad-motion-receive-reopen/1") => true,
            _ => anyhow::bail!("native_motion_request_schema_unsupported"),
        };
        let received = super::bound(root, &i.receive_request, &raw, reopen)?;
        historical_observation = Some(received.value.clone());
        ensure!(
            received.request.object == *object,
            "native_motion_execution_object_differs"
        );
        applicable(&received.project, project, object)?;
        let report: CadMotionReceiveResult1 = serde_json::from_value(received.value)?;
        report
            .validate()
            .map_err(|_| anyhow::anyhow!("native_motion_report_invalid"))?;
        let (conclusion, reason) = if received.request.capture_scope == "binding_facts2" {
            assess(&i.claim, &report)
        } else {
            ("unknown", Some("native_motion_legacy_capture_scope"))
        };
        Ok((
            conclusion,
            reason.map(str::to_owned),
            json!({"claim":i.claim,"request":i.receive_request,
            "applicability":i.applicability,"historical_project":received.request.project,
            "historical_context":received.request.project_context,"current_context":project::context(project, object)?,
            "observation":report,"engineering_adoption":"not_granted"}),
        ))
    })();
    match checked {
        Ok((conclusion, reason, details)) => result(project, a, conclusion, reason, details, true),
        Err(e) => result(
            project,
            a,
            "unknown",
            Some(e.to_string()),
            json!({"claim":a.requirements["claim"],"current_use":"requires_review","historical_observation":historical_observation}),
            historical_observation.is_some(),
        ),
    }
}
