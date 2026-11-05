//! Current realization and declared claim composition, using existing owners.
//! A decomposition is an engineering declaration, not a proof of sufficiency.
use super::{affected, basis, result};
use crate::{
    CheckResult,
    project::{self, Project},
};
use anyhow::{Context, Result, ensure};
use oe_contracts::{
    EngineeringAssertion, EngineeringBasis, EngineeringCheckReference, EngineeringClaimComposition,
    EngineeringFunctionRealization, ObjectIdentity,
};
use oe_core::SkillRoot;
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn predicate(kind: &str) -> bool {
    matches!(
        kind,
        "numeric_acceptance"
            | "mechanism_motion"
            | "cad_native_motion"
            | "cad_row_intent"
            | "cad_drawing_reference"
            | "cad_sheet_reference_persistence"
            | "cad_drafting_sheet"
    )
}

fn leaf(
    root: Option<&SkillRoot>,
    project: &Project,
    reference: &EngineeringCheckReference,
    mechanism: bool,
) -> Result<Vec<CheckResult>> {
    project::context(project, &reference.object)?;
    let b = basis(project, &reference.object)?.context("engineering_relation_basis_missing")?;
    let support = super::basis_support(project, &reference.object, &b);
    ensure!(
        support.conclusion == "pass",
        "engineering_relation_basis_support_not_current:{}",
        support.id
    );
    let a = b
        .assertions
        .iter()
        .find(|a| a.assertion_id == reference.assertion_id)
        .context("engineering_relation_check_unbound")?;
    ensure!(
        if mechanism {
            matches!(a.kind.as_str(), "model_boundary" | "mechanism_motion")
        } else {
            predicate(&a.kind)
        },
        "engineering_relation_predicate_required"
    );
    // Relation-to-relation references are deliberately unsupported: no cycle,
    // declaration-only pass or recursive graph can supply affirmative evidence.
    Ok(match root {
        Some(root) => super::assertion_checks_current(root, project, &b, a, &reference.object),
        None => vec![super::assertion_check(project, &b, a, &reference.object)],
    })
}

fn state(checks: &[CheckResult]) -> &'static str {
    if checks
        .iter()
        .any(|c| c.execution == "completed" && c.conclusion == "violated")
    {
        "violated"
    } else if !checks.is_empty()
        && checks
            .iter()
            .all(|c| c.execution == "completed" && c.conclusion == "pass")
    {
        "pass"
    } else {
        "unknown"
    }
}

fn observation(
    root: Option<&SkillRoot>,
    p: &Project,
    r: &EngineeringCheckReference,
    mechanism: bool,
) -> Value {
    match leaf(root, p, r, mechanism) {
        Ok(checks) => json!({"reference":r,"conclusion":state(&checks),"checks":checks}),
        Err(e) => json!({"reference":r,"conclusion":"unknown","reason":e.to_string()}),
    }
}

fn all(rows: &[Value]) -> &'static str {
    if rows.iter().any(|r| r["conclusion"] == "violated") {
        "violated"
    } else if !rows.is_empty() && rows.iter().all(|r| r["conclusion"] == "pass") {
        "pass"
    } else {
        "unknown"
    }
}

fn realization(
    root: Option<&SkillRoot>,
    p: &Project,
    a: &EngineeringAssertion,
) -> Result<(String, Value)> {
    let req = &a.requirements;
    if req.get("schema").is_none() {
        let function = req["function_id"]
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .context("engineering_function_identity_missing")?;
        let missing = ["current_realization", "mechanism", "verification_plan"]
            .iter()
            .any(|key| req[key].as_str().is_none_or(|s| s.trim().is_empty()));
        let violated = missing && req["inventory_scope"] == "declared_complete";
        let detail = json!({"function_id":function,"current_realization":req["current_realization"],
            "mechanism":req["mechanism"],"verification_plan":req["verification_plan"],"inventory_scope":req["inventory_scope"]});
        return Ok((
            if violated { "violated" } else { "unknown" }.into(),
            json!({
            "reason":if violated {"engineering_necessary_function_unassigned"} else {"engineering_function_binding_required"},
            "mismatches":if violated {vec![detail.clone()]} else {vec![]},
            "unknown":if violated {vec![]} else {vec![detail]},
            "binding":"legacy_text_only","physical_realization":"not_established"}),
        ));
    }
    let r: EngineeringFunctionRealization = serde_json::from_value(req.clone())?;
    r.validate()
        .map_err(|e| anyhow::anyhow!("engineering_realization_invalid:{e:?}"))?;
    ensure!(
        !r.function_id.trim().is_empty(),
        "engineering_function_identity_missing"
    );
    if r.state != "assigned" {
        return Ok((
            "unknown".into(),
            json!({"reason":"engineering_function_not_assigned","allocation":r,
            "physical_realization":"not_established"}),
        ));
    }
    let object = r
        .realization
        .as_ref()
        .context("engineering_realization_object_missing")?;
    project::context(p, object)?;
    let mechanism = r
        .mechanism_assertion
        .as_ref()
        .context("engineering_realization_mechanism_missing")?;
    ensure!(
        !r.verification_assertions.is_empty() && r.verification_assertions.len() <= 32,
        "engineering_realization_verification_missing_or_budget"
    );
    let mut unique = BTreeSet::from([mechanism]);
    ensure!(
        r.verification_assertions.iter().all(|id| unique.insert(id)),
        "engineering_realization_check_duplicate"
    );
    let mut observations = vec![observation(
        root,
        p,
        &EngineeringCheckReference {
            object: object.clone(),
            assertion_id: mechanism.clone(),
        },
        true,
    )];
    observations.extend(r.verification_assertions.iter().map(|id| {
        observation(
            root,
            p,
            &EngineeringCheckReference {
                object: object.clone(),
                assertion_id: id.clone(),
            },
            false,
        )
    }));
    let conclusion = if all(&observations) == "violated" {
        "violated"
    } else if r.inventory_scope == "partial" {
        "unknown"
    } else {
        all(&observations)
    };
    Ok((
        conclusion.into(),
        json!({"allocation":r,"checks":observations,
        "reason":if conclusion=="pass" {Value::Null} else {json!("engineering_realization_obligations_unmet")},
        "support_status":"bound_allocation_and_scoped_checks_only","physical_realization":"not_certified"}),
    ))
}

fn composition(
    root: Option<&SkillRoot>,
    p: &Project,
    b: &EngineeringBasis,
    a: &EngineeringAssertion,
    object: &ObjectIdentity,
) -> Result<(String, Value)> {
    let r: EngineeringClaimComposition = serde_json::from_value(a.requirements.clone())?;
    r.validate()
        .map_err(|e| anyhow::anyhow!("engineering_claim_invalid:{e:?}"))?;
    ensure!(
        &r.object == object && a.subject == r.decision_id && b.acceptance.contains(&r.criterion),
        "engineering_claim_subject_or_criterion_mismatch"
    );
    project::context(p, object)?;
    let decision = p
        .decisions
        .iter()
        .find(|d| d.id == r.decision_id)
        .context("engineering_claim_decision_missing")?;
    ensure!(
        decision.object_id == object.object_id
            && decision.object_revision == object.revision
            && decision.condition_scope == object.condition_scope
            && decision.source_sha256 == object.source_sha256,
        "engineering_claim_decision_binding_mismatch"
    );
    ensure!(
        a.affected_decisions.contains(&r.decision_id),
        "engineering_claim_affected_decision_missing"
    );
    let count = r.all_of.len()
        + r.interfaces.len()
        + r.any_of.iter().map(|g| g.alternatives.len()).sum::<usize>();
    ensure!(
        count > 0 && count <= 64 && r.any_of.iter().all(|g| !g.alternatives.is_empty()),
        "engineering_claim_obligation_budget_or_empty"
    );
    ensure!(
        !r.all_of.is_empty() || !r.any_of.is_empty(),
        "engineering_claim_components_missing"
    );
    let mut unique = BTreeSet::new();
    for reference in r
        .all_of
        .iter()
        .chain(&r.interfaces)
        .chain(r.any_of.iter().flat_map(|g| &g.alternatives))
    {
        ensure!(
            unique.insert(serde_json::to_string(reference)?),
            "engineering_claim_duplicate_obligation"
        );
    }
    let required: Vec<_> = r
        .all_of
        .iter()
        .map(|x| observation(root, p, x, false))
        .collect();
    let interfaces: Vec<_> = r
        .interfaces
        .iter()
        .map(|x| observation(root, p, x, false))
        .collect();
    let alternatives: Vec<_> = r
        .any_of
        .iter()
        .map(|g| {
            let rows: Vec<_> = g
                .alternatives
                .iter()
                .map(|x| observation(root, p, x, false))
                .collect();
            let conclusion = if rows.iter().any(|r| r["conclusion"] == "pass") {
                "pass"
            } else if rows.iter().all(|r| r["conclusion"] == "violated") {
                "violated"
            } else {
                "unknown"
            };
            json!({"conclusion":conclusion,"alternatives":rows})
        })
        .collect();
    let mut rows: Vec<_> = required
        .iter()
        .chain(&interfaces)
        .chain(&alternatives)
        .cloned()
        .collect();
    let mut missing = Vec::new();
    if r.scope == "system" && r.interfaces.is_empty() {
        missing.push("system_interface_or_aggregate_check");
    }
    if r.coverage != "declared_complete" {
        missing.push("complete_decomposition");
    }
    for what in &missing {
        rows.push(json!({"conclusion":"unknown","missing":what}));
    }
    let conclusion = all(&rows);
    Ok((
        conclusion.into(),
        json!({"claim":r,"all_of":required,"any_of":alternatives,"interfaces":interfaces,
        "missing_obligations":missing,"reason":if conclusion=="pass" {Value::Null} else {json!("engineering_claim_obligations_unmet")},
        "support_status":"declared_check_composition_only","decomposition_sufficiency":"requires_engineering_review",
        "physical_truth":"not_certified"}),
    ))
}

pub(super) fn check(
    root: Option<&SkillRoot>,
    p: &Project,
    b: &EngineeringBasis,
    a: &EngineeringAssertion,
    object: &ObjectIdentity,
) -> CheckResult {
    let attempted = (|| -> Result<(String, Value)> {
        let current = basis(p, object)?.context("engineering_relation_basis_missing")?;
        let support = super::basis_support(p, object, &current);
        if support.conclusion != "pass" {
            return Ok((
                "unknown".into(),
                json!({"reason":"engineering_relation_basis_support_not_current","context_check":support}),
            ));
        }
        if a.kind == "function_continuity" {
            realization(root, p, a)
        } else {
            composition(root, p, b, a, object)
        }
    })();
    let (conclusion, mut details) = attempted.unwrap_or_else(|e| {
        (
            "unknown".into(),
            json!({"reason":e.to_string(),"relation":"not_established"}),
        )
    });
    details["affected_decisions"] = json!(affected(p, a));
    details["subject"] = json!(a.subject);
    details["kind"] = json!(a.kind);
    let reason = details["reason"].as_str().map(str::to_owned);
    result(
        &format!("engineering:{}", a.assertion_id),
        &conclusion,
        reason,
        details,
    )
}
