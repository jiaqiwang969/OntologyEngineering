//! Explicit, one-hop contribution declarations in the existing Project.
//! A consistent relation establishes no physical or parent-goal achievement.
use super::{basis, result};
use crate::{CheckResult, project::Project};
use oe_contracts::{EngineeringBasis, EngineeringBasisV2, EngineeringGoalSupport, ObjectIdentity};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const SUPPORT_SCHEMA: &str = "oev2.engineering-goal-support/1";

fn issue(out: &mut Vec<Value>, reason: &str, details: Value) {
    out.push(json!({"reason":reason,"details":details}));
}

fn scope_affected(
    project: &Project,
    object: &ObjectIdentity,
    current: &EngineeringBasis,
    support: Option<&EngineeringGoalSupport>,
    global: bool,
) -> Vec<String> {
    let mut seeds = BTreeSet::from([object.object_id.clone(), object.condition_scope.clone()]);
    for assertion in &current.assertions {
        seeds.extend(assertion.affected_decisions.iter().cloned());
    }
    if global {
        seeds.extend(crate::project::purpose_changes(&[project]));
    }
    if let Some(support) = support {
        seeds.extend(support.affected_decisions.iter().cloned());
        seeds.insert(support.parent_object.object_id.clone());
        seeds.insert(support.parent_object.condition_scope.clone());
        seeds.extend(
            support
                .sources
                .iter()
                .map(|source| source.source_ref.clone()),
        );
    }
    let mut affected: BTreeSet<String> = project
        .decisions
        .iter()
        .filter(|d| seeds.contains(&d.id))
        .map(|d| d.id.clone())
        .collect();
    if let Ok(impact) = crate::project::impact(project, &seeds) {
        affected.extend(
            impact["decisions"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|d| d["decision_id"].as_str().map(str::to_owned)),
        );
    }
    affected.into_iter().collect()
}

fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

/// Canonicalize the seven original fields, retaining their actual /2 schema.
/// The support envelope is excluded to avoid a recursive digest.
fn body_identity(raw: &Value) -> anyhow::Result<String> {
    let mut body = serde_json::Map::new();
    for key in [
        "schema",
        "object_id",
        "object_revision",
        "purpose",
        "acceptance",
        "facts",
        "assertions",
    ] {
        body.insert(
            key.into(),
            raw.get(key)
                .ok_or_else(|| anyhow::anyhow!("engineering_goal_local_basis_missing"))?
                .clone(),
        );
    }
    Ok(oe_core::sha256(oe_wire::json::canonical_json(
        &Value::Object(body),
    )?))
}

/// Production checks the explicit relationship, not whether a textual
/// contribution entails its parent goal or whether an obligation was fulfilled.
pub(super) fn check(
    project: &Project,
    object: &ObjectIdentity,
    current: &EngineeringBasis,
    raw: &Value,
) -> CheckResult {
    let mut conflicts = Vec::new();
    let mut unknown = Vec::new();
    let mut parent_acceptance = Vec::<String>::new();
    let mut parent_available = false;
    let mut parent_unknown = Vec::new();
    let mut actual_local_hash = None;
    let support_raw = raw.get("goal_support");
    if let Some(scope) = support_raw
        .and_then(|v| v.get("claim_scope"))
        .and_then(Value::as_str)
    {
        if !scope.trim().is_empty() && scope != "local_obligation_only" {
            issue(
                &mut conflicts,
                "engineering_goal_scope_broadened",
                json!({"actual":scope,"required":"local_obligation_only"}),
            );
        }
    }
    let support: Option<EngineeringGoalSupport> = match support_raw {
        None => {
            issue(&mut unknown, "engineering_goal_support_missing", json!({}));
            None
        }
        Some(value) => match serde_json::from_value::<EngineeringGoalSupport>(value.clone()) {
            Ok(support) => Some(support),
            Err(error) => {
                issue(
                    &mut unknown,
                    "engineering_goal_support_invalid",
                    json!({"diagnostic":error.to_string()}),
                );
                None
            }
        },
    };
    if let Some(support) = &support {
        if support.validate().is_err()
            || support.schema != SUPPORT_SCHEMA
            || support.local_object.validate().is_err()
            || support.parent_object.validate().is_err()
            || !digest(&support.parent_condition_sha256)
            || !digest(&support.local_basis_sha256)
        {
            issue(
                &mut unknown,
                "engineering_goal_support_invalid",
                json!({"schema":support.schema}),
            );
        }
        if !serde_json::from_value::<EngineeringBasisV2>(raw.clone())
            .map(|b| b.validate().is_ok())
            .unwrap_or(false)
        {
            issue(
                &mut unknown,
                "engineering_goal_support_invalid",
                json!({"full_contract":"invalid"}),
            );
        }
        if support.obligations.len() > 64
            || support.sources.len() > 256
            || support.affected_decisions.len() > 256
        {
            issue(
                &mut unknown,
                "engineering_goal_support_budget",
                json!({"obligations":support.obligations.len(),"sources":support.sources.len(),"affected_decisions":support.affected_decisions.len()}),
            );
        } else {
            if (!support.project_id.trim().is_empty() && support.project_id != project.project_id)
                || (!support.project_revision.trim().is_empty()
                    && support.project_revision != project.revision)
                || (!support.project_purpose.trim().is_empty()
                    && support.project_purpose != project.purpose)
            {
                issue(
                    &mut conflicts,
                    "engineering_goal_project_changed",
                    json!({"expected":{"project_id":support.project_id,"revision":support.project_revision,"purpose":support.project_purpose},"current":{"project_id":project.project_id,"revision":project.revision,"purpose":project.purpose}}),
                );
            }
            if support.local_object.validate().is_ok()
                && digest(&support.local_object.source_sha256)
                && (&support.local_object != object
                    || project.objects.get(&object.object_id) != Some(&object.revision)
                    || !project
                        .evidence
                        .values()
                        .any(|hash| hash == &object.source_sha256))
            {
                issue(
                    &mut conflicts,
                    "engineering_goal_local_identity_changed",
                    json!({"expected":support.local_object,"current":object}),
                );
            }
            match body_identity(raw) {
                Ok(hash) => {
                    if digest(&support.local_basis_sha256) && hash != support.local_basis_sha256 {
                        issue(
                            &mut conflicts,
                            "engineering_goal_local_basis_changed",
                            json!({"expected":support.local_basis_sha256,"actual":hash,"algorithm":"canonical_json of seven original fields including schema/2"}),
                        );
                    }
                    actual_local_hash = Some(hash);
                }
                Err(error) => issue(
                    &mut unknown,
                    "engineering_goal_local_basis_missing",
                    json!({"diagnostic":error.to_string()}),
                ),
            }
            let mut source_ids = BTreeSet::new();
            for source in &support.sources {
                if source.validate().is_err()
                    || !source_ids.insert(source.source_ref.clone())
                    || !digest(&source.source_sha256)
                {
                    issue(
                        &mut unknown,
                        "engineering_goal_source_invalid",
                        json!({"source":source}),
                    );
                }
                if !source.source_ref.trim().is_empty() && digest(&source.source_sha256) {
                    match project.evidence.get(&source.source_ref) {
                        Some(hash) if hash == &source.source_sha256 => (),
                        Some(hash) => issue(
                            &mut conflicts,
                            "engineering_goal_source_changed",
                            json!({"source_ref":source.source_ref,"expected":source.source_sha256,"current":hash}),
                        ),
                        None => issue(
                            &mut conflicts,
                            "engineering_goal_reference_unbound",
                            json!({"source_ref":source.source_ref}),
                        ),
                    }
                }
            }
            for identity in [object, &support.parent_object] {
                if !support.sources.iter().any(|s| {
                    s.source_sha256 == identity.source_sha256
                        && project.evidence.get(&s.source_ref) == Some(&s.source_sha256)
                }) {
                    issue(
                        &mut unknown,
                        "engineering_goal_source_coverage_missing",
                        json!({"object":identity}),
                    );
                }
            }
            for source_ref in current
                .facts
                .iter()
                .flat_map(|f| &f.source_refs)
                .chain(current.assertions.iter().flat_map(|a| &a.source_refs))
            {
                if !source_ids.contains(source_ref) {
                    issue(
                        &mut unknown,
                        "engineering_goal_source_coverage_missing",
                        json!({"local_source_ref":source_ref}),
                    );
                }
            }
            if support.parent_object.validate().is_ok()
                && digest(&support.parent_object.source_sha256)
                && (support.parent_object.object_id == object.object_id
                    || support.parent_object.condition_scope == object.condition_scope
                    || project.objects.get(&support.parent_object.object_id)
                        != Some(&support.parent_object.revision)
                    || !project
                        .evidence
                        .values()
                        .any(|hash| hash == &support.parent_object.source_sha256))
            {
                issue(
                    &mut conflicts,
                    "engineering_goal_parent_identity_changed",
                    json!({"parent":support.parent_object,"current_revision":project.objects.get(&support.parent_object.object_id)}),
                );
            }
            match project
                .conditions
                .get(&support.parent_object.condition_scope)
            {
                None => {
                    // Missing data remains unknown. A comparable original parent
                    // body still present under a different current scope proves a
                    // wrong reference rather than an unavailable parent input.
                    let actual_scopes: Vec<_> = if support.parent_object.validate().is_ok()
                        && digest(&support.parent_condition_sha256)
                    {
                        project
                            .conditions
                            .iter()
                            .filter_map(|(scope, condition)| {
                                if oe_core::sha256(condition.as_bytes())
                                    != support.parent_condition_sha256
                                {
                                    return None;
                                }
                                let value =
                                    oe_core::json::strict_json(condition.as_bytes()).ok()?;
                                (value["object_id"] == support.parent_object.object_id
                                    && value["object_revision"] == support.parent_object.revision)
                                    .then(|| scope.clone())
                            })
                            .collect()
                    } else {
                        vec![]
                    };
                    if actual_scopes.is_empty() {
                        issue(
                            &mut unknown,
                            "engineering_goal_parent_missing",
                            json!({"scope":support.parent_object.condition_scope}),
                        );
                    } else {
                        issue(
                            &mut conflicts,
                            "engineering_goal_parent_identity_changed",
                            json!({"declared_scope":support.parent_object.condition_scope,"actual_scopes":actual_scopes,"bound_condition_sha256":support.parent_condition_sha256}),
                        );
                    }
                }
                Some(parent_raw) => {
                    let actual = oe_core::sha256(parent_raw.as_bytes());
                    if digest(&support.parent_condition_sha256)
                        && actual != support.parent_condition_sha256
                    {
                        issue(
                            &mut conflicts,
                            "engineering_goal_parent_condition_changed",
                            json!({"expected":support.parent_condition_sha256,"actual":actual,"algorithm":"raw UTF8 condition bytes"}),
                        );
                    }
                    match oe_core::json::strict_json(parent_raw.as_bytes()) {
                        Ok(parent) if parent["schema"] != super::SCHEMA => issue(
                            &mut unknown,
                            "engineering_goal_parent_version_unsupported",
                            json!({"schema":parent["schema"],"supported":"one-hop engineering-basis/1"}),
                        ),
                        Ok(_) => match basis(project, &support.parent_object) {
                            Ok(Some(parent)) => {
                                parent_available = true;
                                if parent.purpose != project.purpose {
                                    issue(
                                        &mut conflicts,
                                        "engineering_goal_parent_purpose_changed",
                                        json!({"parent_purpose":parent.purpose,"project_purpose":project.purpose}),
                                    );
                                }
                                parent_acceptance = parent.acceptance.clone();
                                parent_unknown = parent
                                    .facts
                                    .iter()
                                    .filter(|f| f.status != "declared" || f.value.is_null())
                                    .map(|f| json!(f))
                                    .collect();
                                for source_ref in
                                    parent.facts.iter().flat_map(|f| &f.source_refs).chain(
                                        parent.assertions.iter().flat_map(|a| &a.source_refs),
                                    )
                                {
                                    if !source_ids.contains(source_ref) {
                                        issue(
                                            &mut unknown,
                                            "engineering_goal_source_coverage_missing",
                                            json!({"parent_source_ref":source_ref}),
                                        );
                                    }
                                }
                            }
                            Ok(None) => {
                                issue(&mut unknown, "engineering_goal_parent_missing", json!({}))
                            }
                            Err(error) => issue(
                                &mut unknown,
                                "engineering_goal_parent_invalid",
                                json!({"diagnostic":error.to_string()}),
                            ),
                        },
                        Err(error) => issue(
                            &mut unknown,
                            "engineering_goal_parent_invalid",
                            json!({"diagnostic":error.to_string()}),
                        ),
                    }
                }
            }
            if support.obligations.is_empty()
                || current.acceptance.is_empty()
                || parent_acceptance.is_empty()
            {
                issue(
                    &mut unknown,
                    "engineering_goal_obligations_missing",
                    json!({"local_acceptance":current.acceptance,"parent_acceptance":parent_acceptance}),
                );
            }
            let mut ids = BTreeSet::new();
            let mut coverage = BTreeMap::<String, usize>::new();
            for obligation in &support.obligations {
                if obligation.validate().is_err() || !ids.insert(obligation.obligation_id.clone()) {
                    issue(
                        &mut unknown,
                        "engineering_goal_obligation_invalid",
                        json!({"obligation":obligation}),
                    );
                }
                *coverage
                    .entry(obligation.local_acceptance.clone())
                    .or_default() += 1;
                if (!obligation.local_acceptance.trim().is_empty()
                    && !current.acceptance.contains(&obligation.local_acceptance))
                    || (parent_available
                        && !obligation.parent_acceptance.trim().is_empty()
                        && !parent_acceptance.contains(&obligation.parent_acceptance))
                {
                    issue(
                        &mut conflicts,
                        "engineering_goal_acceptance_changed",
                        json!({"obligation_id":obligation.obligation_id,"local":obligation.local_acceptance,"parent":obligation.parent_acceptance}),
                    );
                }
                if obligation.contribution.trim().is_empty() {
                    issue(
                        &mut unknown,
                        "engineering_goal_contribution_missing",
                        json!({"obligation_id":obligation.obligation_id}),
                    );
                }
                if obligation.assertion_ids.is_empty() && obligation.fact_properties.is_empty() {
                    issue(
                        &mut unknown,
                        "engineering_goal_reference_missing",
                        json!({"obligation_id":obligation.obligation_id}),
                    );
                }
                let mut references = BTreeSet::new();
                for id in &obligation.assertion_ids {
                    if !references.insert(format!("assertion:{id}")) {
                        issue(
                            &mut unknown,
                            "engineering_goal_reference_ambiguous",
                            json!({"assertion_id":id}),
                        );
                    }
                    match current.assertions.iter().find(|a| &a.assertion_id == id) {
                        None => issue(
                            &mut conflicts,
                            "engineering_goal_reference_unbound",
                            json!({"assertion_id":id,"obligation_id":obligation.obligation_id}),
                        ),
                        Some(a) => {
                            for source_ref in &a.source_refs {
                                if !source_ids.contains(source_ref) {
                                    issue(
                                        &mut unknown,
                                        "engineering_goal_source_coverage_missing",
                                        json!({"assertion_source_ref":source_ref}),
                                    );
                                }
                            }
                        }
                    }
                }
                for property in &obligation.fact_properties {
                    if !references.insert(format!("fact:{property}")) {
                        issue(
                            &mut unknown,
                            "engineering_goal_reference_ambiguous",
                            json!({"fact_property":property}),
                        );
                    }
                    match current.facts.iter().find(|f| &f.property == property) {
                        None => issue(
                            &mut conflicts,
                            "engineering_goal_reference_unbound",
                            json!({"fact_property":property,"obligation_id":obligation.obligation_id}),
                        ),
                        Some(f) => {
                            for source_ref in &f.source_refs {
                                if !source_ids.contains(source_ref) {
                                    issue(
                                        &mut unknown,
                                        "engineering_goal_source_coverage_missing",
                                        json!({"fact_source_ref":source_ref}),
                                    );
                                }
                            }
                        }
                    }
                }
            }
            for acceptance in &current.acceptance {
                match coverage.get(acceptance) {
                    None => issue(
                        &mut unknown,
                        "engineering_goal_obligations_missing",
                        json!({"unmapped_local_acceptance":acceptance}),
                    ),
                    Some(1) => (),
                    Some(count) => issue(
                        &mut conflicts,
                        "engineering_goal_acceptance_ambiguous",
                        json!({"local_acceptance":acceptance,"count":count}),
                    ),
                }
            }
            for id in &support.affected_decisions {
                if !project.decisions.iter().any(|d| &d.id == id) {
                    issue(
                        &mut conflicts,
                        "engineering_goal_reference_unbound",
                        json!({"affected_decision":id}),
                    );
                }
            }
        }
    }
    let conclusion = if !conflicts.is_empty() {
        "violated"
    } else if !unknown.is_empty() {
        "unknown"
    } else {
        "pass"
    };
    let global = conflicts.iter().any(|issue| {
        issue["reason"] == "engineering_goal_project_changed"
            || issue["reason"] == "engineering_goal_parent_purpose_changed"
    });
    let affected = if conclusion == "pass" {
        vec![]
    } else {
        scope_affected(project, object, current, support.as_ref(), global)
    };
    let reason = conflicts
        .first()
        .or(unknown.first())
        .and_then(|v| v["reason"].as_str())
        .map(str::to_owned);
    let mut out = result(
        "engineering_goal_support",
        conclusion,
        reason,
        json!({
            "schema":"oev2.engineering-goal-relation-check/1","local_purpose":current.purpose,"current_project_purpose":project.purpose,
            "support":support_raw,"actual_local_basis_sha256":actual_local_hash,"conflicts":conflicts,"unknown":unknown,"affected_decisions":affected,
            "remaining_parent_obligations":parent_acceptance.iter().map(|acceptance|json!({"acceptance":acceptance,"achievement":"not_established","contribution_is_not_fulfillment":true})).collect::<Vec<_>>(),
            "unresolved_local_facts":current.facts.iter().filter(|f|f.status!="declared" || f.value.is_null()).collect::<Vec<_>>(),"unresolved_parent_facts":parent_unknown,
            "relation_status":if conclusion=="pass" {"declared_contribution_consistent"} else {"requires_review"},
            "source_binding":"current Project evidence declarations","external_original_source_bytes":"not_checked",
            "local_obligation_fulfillment":"not_established","total_goal_achievement":"not_established","physical_truth":"not_established"
        }),
    );
    out.failures = conflicts
        .iter()
        .chain(&unknown)
        .filter_map(|v| v["reason"].as_str().map(str::to_owned))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    out
}
