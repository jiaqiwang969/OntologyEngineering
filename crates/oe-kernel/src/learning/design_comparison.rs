//! Declared-model comparison remains under the existing learning/selection owner.
//! No project mutation, author scores, native execution or method promotion.
use crate::{engineering, project::Project};
use anyhow::{Context, Result, ensure};
use oe_contracts::{
    DesignComparisonRule, EngineeringCalculation, EngineeringDesignComparisonRequest,
    ObjectIdentity, Quantity,
};
use oe_core::SkillRoot;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub struct Side<'a> {
    pub object: &'a ObjectIdentity,
    pub calculations: &'a [EngineeringCalculation],
    pub execution: &'a str,
}

fn validate_dto(result: std::result::Result<(), Vec<String>>) -> Result<()> {
    result.map_err(|_| anyhow::anyhow!("design_comparison_contract_invalid"))
}
fn nonempty(s: &str) -> bool {
    !s.trim().is_empty()
}
fn number(value: &Value) -> Result<(f64, String)> {
    let q: Quantity = serde_json::from_value(value.clone())?;
    validate_dto(q.validate())?;
    ensure!(
        q.value.is_finite() && nonempty(&q.unit),
        "design_comparison_quantity_invalid"
    );
    Ok((q.value, q.unit))
}
fn comparable(value: &Value) -> Value {
    if let Some(object) = value.as_object() {
        if object.contains_key("value") && object.contains_key("unit") {
            return json!({"value":value["value"],"unit":value["unit"]});
        }
    }
    value.clone()
}
fn calculation<'a>(s: &'a Side<'_>, id: &Option<String>) -> Option<&'a EngineeringCalculation> {
    s.calculations
        .iter()
        .find(|c| Some(&c.calculation_id) == id.as_ref())
}
fn permitted_variable(method: &str, property: &str) -> bool {
    match method {
        "beam-cantilever-end-load/1" | "beam-simple-center-load/1" => property == "second_moment",
        "round-shaft-second-moment/1" => property == "diameter",
        "coulomb-single-contact-limit/1" | "coulomb-required-friction/1" => {
            property == "normal_force"
        }
        // Speed, load and coefficient are not silently reclassified as design choices.
        _ => false,
    }
}

fn goal(
    root: &SkillRoot,
    project: &Project,
    parent: &ObjectIdentity,
    side: &Side<'_>,
) -> Result<()> {
    if side.object == parent {
        return Ok(());
    }
    let raw: Value = oe_core::json::strict_json(
        project
            .conditions
            .get(&side.object.condition_scope)
            .context("design_comparison_child_condition_missing")?
            .as_bytes(),
    )?;
    ensure!(
        raw["schema"] == "oev2.engineering-basis/2"
            && raw["goal_support"]["parent_object"] == json!(parent),
        "design_comparison_parent_goal_unbound"
    );
    let checks = engineering::checks_current(root, project, side.object);
    ensure!(
        checks.iter().any(|c| c.id == "engineering_goal_support"
            && c.execution == "completed"
            && c.conclusion == "pass"),
        "design_comparison_parent_goal_invalid"
    );
    Ok(())
}

/// Validate the frozen common task and the explicitly bounded design changes.
/// Unknown calculations may be recorded; changed rules or external conditions may not.
pub fn validate(
    root: &SkillRoot,
    project: &Project,
    parent: &ObjectIdentity,
    acceptance: &[String],
    input: &EngineeringDesignComparisonRequest,
    baseline: &Side<'_>,
    candidate: &Side<'_>,
) -> Result<()> {
    validate_dto(input.validate())?;
    ensure!(
        input.schema == "oev2.design-comparison/1"
            && input.exposure == "development_replay"
            && nonempty(&input.comparison_id)
            && nonempty(&input.source_family),
        "design_comparison_identity_or_exposure_invalid"
    );
    validate_rules(
        root,
        project,
        parent,
        acceptance,
        &input.rules,
        baseline,
        candidate,
    )
}

/// The unchanged controlled-model policy also serves general Project/work
/// exploration. Supplier attempt preparation remains owned by its adapter.
pub fn validate_rules(
    root: &SkillRoot,
    project: &Project,
    parent: &ObjectIdentity,
    acceptance: &[String],
    rules: &[DesignComparisonRule],
    baseline: &Side<'_>,
    candidate: &Side<'_>,
) -> Result<()> {
    ensure!(
        !acceptance.is_empty()
            && acceptance.len() <= 64
            && acceptance.iter().all(|s| nonempty(s))
            && acceptance.iter().collect::<BTreeSet<_>>().len() == acceptance.len(),
        "design_comparison_acceptance_invalid"
    );
    ensure!(
        !rules.is_empty() && rules.len() <= 64,
        "design_comparison_rule_budget"
    );
    goal(root, project, parent, baseline)?;
    goal(root, project, parent, candidate)?;
    let before =
        engineering::basis(project, baseline.object)?.context("design_comparison_basis_missing")?;
    let after = engineering::basis(project, candidate.object)?
        .context("design_comparison_basis_missing")?;
    let parent_basis =
        engineering::basis(project, parent)?.context("design_comparison_parent_basis_missing")?;
    ensure!(
        parent_basis.acceptance == acceptance,
        "design_comparison_parent_acceptance_changed"
    );
    ensure!(
        before.acceptance == acceptance && after.acceptance == acceptance,
        "design_comparison_acceptance_changed"
    );
    ensure!(
        before.purpose == after.purpose,
        "design_comparison_child_purpose_changed"
    );
    // A common external condition cannot be changed identically on both sides
    // and thereby escape the pairwise comparison.
    for fact in &parent_basis.facts {
        for child in [&before, &after] {
            let current = child
                .facts
                .iter()
                .find(|f| f.property == fact.property)
                .context("design_comparison_parent_condition_missing")?;
            ensure!(
                current.status == fact.status
                    && comparable(&current.value) == comparable(&fact.value)
                    && current.source_refs == fact.source_refs,
                "design_comparison_parent_condition_changed"
            );
        }
    }
    ensure!(
        json!(
            before
                .assertions
                .iter()
                .filter(|a| a.kind != "model_boundary")
                .collect::<Vec<_>>()
        ) == json!(
            after
                .assertions
                .iter()
                .filter(|a| a.kind != "model_boundary")
                .collect::<Vec<_>>()
        ),
        "design_comparison_other_premises_changed"
    );
    let mut ids = BTreeSet::new();
    let mut variables: BTreeMap<&str, Value> = BTreeMap::new();
    for rule in rules {
        validate_dto(rule.validate())?;
        ensure!(
            nonempty(&rule.rule_id)
                && ids.insert(&rule.rule_id)
                && acceptance.contains(&rule.acceptance),
            "design_comparison_rule_unbound_or_duplicate"
        );
        if rule.owner == "not_implemented" {
            ensure!(
                rule.baseline_calculation.is_none()
                    && rule.candidate_calculation.is_none()
                    && rule.symbol.is_none()
                    && rule.unit.is_none()
                    && rule.direction.is_none()
                    && rule.minimum_improvement.is_none()
                    && rule.design_variables.is_empty(),
                "design_comparison_unimplemented_rule_payload"
            );
            continue;
        }
        ensure!(
            rule.owner == "calculated_model"
                && rule.baseline_calculation.as_deref().is_some_and(nonempty)
                && rule.candidate_calculation.as_deref().is_some_and(nonempty)
                && rule.symbol.as_deref().is_some_and(nonempty)
                && rule.unit.as_deref().is_some_and(nonempty)
                && matches!(rule.direction.as_deref(), Some("lower" | "higher"))
                && rule
                    .minimum_improvement
                    .is_some_and(|v| v.is_finite() && v >= 0.0)
                && rule.design_variables.len() <= 8,
            "design_comparison_model_rule_invalid"
        );
        let a = calculation(baseline, &rule.baseline_calculation);
        let b = calculation(candidate, &rule.candidate_calculation);
        let mut keys = BTreeSet::new();
        for variable in &rule.design_variables {
            validate_dto(variable.validate())?;
            validate_dto(variable.lower.validate())?;
            validate_dto(variable.upper.validate())?;
            ensure!(
                keys.insert(variable.property.as_str())
                    && nonempty(&variable.property)
                    && variable.lower.value.is_finite()
                    && variable.upper.value.is_finite()
                    && variable.lower.value <= variable.upper.value
                    && variable.lower.unit == variable.upper.unit,
                "design_comparison_variable_invalid"
            );
            for c in [a, b].into_iter().flatten() {
                ensure!(
                    permitted_variable(&c.method, &variable.property),
                    "design_comparison_external_condition_not_design_variable"
                );
                let (v, unit) = number(&c.inputs[&variable.property])?;
                ensure!(
                    unit == variable.lower.unit
                        && v >= variable.lower.value
                        && v <= variable.upper.value,
                    "design_comparison_variable_outside_frozen_range"
                );
            }
            // Every rule that uses a variable must use the same frozen range.
            let value = json!(variable);
            if let Some(old) = variables.insert(&variable.property, value.clone()) {
                ensure!(old == value, "design_comparison_variable_ranges_conflict");
            }
        }
        if let (Some(a), Some(b)) = (a, b) {
            ensure!(
                a.object == *baseline.object
                    && b.object == *candidate.object
                    && a.method == b.method
                    && a.assumptions == b.assumptions
                    && a.validity_conditions == b.validity_conditions
                    && a.invalid_when == b.invalid_when
                    && a.source_refs == b.source_refs,
                "design_comparison_model_or_premises_changed"
            );
            let ai = a
                .inputs
                .as_object()
                .context("design_comparison_inputs_invalid")?;
            let bi = b
                .inputs
                .as_object()
                .context("design_comparison_inputs_invalid")?;
            ensure!(
                ai.keys().eq(bi.keys()),
                "design_comparison_input_coverage_changed"
            );
            for (key, value) in ai {
                ensure!(
                    keys.contains(key.as_str()) || comparable(value) == comparable(&bi[key]),
                    "design_comparison_external_input_changed:{key}"
                );
            }
        }
    }
    ensure!(
        acceptance
            .iter()
            .all(|a| rules.iter().any(|r| &r.acceptance == a)),
        "design_comparison_acceptance_omitted"
    );
    let af: BTreeMap<_, _> = before.facts.iter().map(|f| (&f.property, f)).collect();
    let bf: BTreeMap<_, _> = after.facts.iter().map(|f| (&f.property, f)).collect();
    ensure!(
        af.keys().eq(bf.keys()),
        "design_comparison_condition_coverage_changed"
    );
    for (key, a) in af {
        let b = bf[key];
        ensure!(
            a.status == b.status && a.source_refs == b.source_refs,
            "design_comparison_condition_status_or_source_changed"
        );
        if let Some(range) = variables.get(key.as_str()) {
            for fact in [a, b] {
                if fact.status == "declared" && !fact.value.is_null() {
                    let (v, unit) = number(&fact.value)?;
                    ensure!(
                        unit == range["lower"]["unit"]
                            && v >= range["lower"]["value"].as_f64().context("variable_lower")?
                            && v <= range["upper"]["value"].as_f64().context("variable_upper")?,
                        "design_comparison_fact_outside_frozen_range"
                    );
                }
            }
        } else {
            ensure!(
                comparable(&a.value) == comparable(&b.value),
                "design_comparison_external_condition_changed:{key}"
            );
        }
    }
    Ok(())
}

fn execute(
    project: &Project,
    rule: &DesignComparisonRule,
    side: &Side<'_>,
    baseline: bool,
) -> Value {
    let unavailable = |execution: &str, reason: &str| {
        json!({"execution":execution,"conclusion":"unknown",
        "reason":reason,"arithmetic":null,"model_applicability":null,"value":null,"unit":null})
    };
    if side.execution != "completed" {
        return unavailable("failed", "design_attempt_execution_failed");
    }
    if rule.owner == "not_implemented" {
        return unavailable(
            "not_run",
            "evidence_owner_not_implemented_for_this_obligation",
        );
    }
    let Some(c) = calculation(
        side,
        if baseline {
            &rule.baseline_calculation
        } else {
            &rule.candidate_calculation
        },
    ) else {
        return unavailable("not_run", "required_calculation_absent");
    };
    let applicability = engineering::calculation_check(project, c);
    let arithmetic = match crate::calculation::calculate(c) {
        Ok(v) => v,
        Err(e) => {
            return json!({"execution":"failed","conclusion":"unknown","reason":e.to_string(),
            "arithmetic":null,"model_applicability":applicability,"value":null,"unit":null});
        }
    };
    let supported = applicability.execution == "completed"
        && applicability.conclusion == "pass"
        && arithmetic["execution"] == "completed"
        && arithmetic["symbol"].as_str() == rule.symbol.as_deref()
        && arithmetic["result"]["unit"].as_str() == rule.unit.as_deref()
        && arithmetic["result"]["value"]
            .as_f64()
            .is_some_and(f64::is_finite);
    json!({"execution":arithmetic["execution"],"conclusion":if supported{"model_result_supported"}else{"unknown"},
        "reason":if supported{Value::Null}else{json!("current_model_or_metric_not_supported")},
        "arithmetic":arithmetic,"model_applicability":applicability,
        "value":if supported{arithmetic["result"]["value"].clone()}else{Value::Null},
        "unit":if supported{arithmetic["result"]["unit"].clone()}else{Value::Null}})
}

pub fn assess(
    root: &SkillRoot,
    project: &Project,
    parent: &ObjectIdentity,
    acceptance: &[String],
    input: &EngineeringDesignComparisonRequest,
    baseline: &Side<'_>,
    candidate: &Side<'_>,
) -> Result<Value> {
    validate(
        root, project, parent, acceptance, input, baseline, candidate,
    )?;
    let mut rows = Vec::new();
    let mut improved = 0usize;
    let mut unchanged = 0usize;
    let mut regressed = 0usize;
    let mut unknown = 0usize;
    let mut baseline_failed = 0usize;
    let mut candidate_failed = 0usize;
    for rule in &input.rules {
        let a = execute(project, rule, baseline, true);
        let b = execute(project, rule, candidate, false);
        baseline_failed += usize::from(a["execution"] == "failed");
        candidate_failed += usize::from(b["execution"] == "failed");
        let (comparison, difference, gain) = match (a["value"].as_f64(), b["value"].as_f64()) {
            (Some(a), Some(b)) if (b - a).is_finite() => {
                let gain = if rule.direction.as_deref() == Some("lower") {
                    a - b
                } else {
                    b - a
                };
                if gain < 0.0 {
                    regressed += 1;
                    ("regressed", json!(b - a), json!(gain))
                } else if gain > rule.minimum_improvement.unwrap_or(f64::INFINITY) {
                    improved += 1;
                    ("improved", json!(b - a), json!(gain))
                } else {
                    unchanged += 1;
                    ("unchanged", json!(b - a), json!(gain))
                }
            }
            _ => {
                unknown += 1;
                ("unknown", Value::Null, Value::Null)
            }
        };
        rows.push(
            json!({"rule_id":rule.rule_id,"acceptance":rule.acceptance,"owner":rule.owner,
            "baseline":a,"candidate":b,"comparison":comparison,"difference":difference,"gain":gain,
            "scope":"declared_model_only","engineering_acceptance":"not_established"}),
        );
    }
    let verdict = if regressed > 0 {
        "regressed"
    } else if unknown > 0 {
        "inconclusive"
    } else if improved > 0 {
        "improved"
    } else {
        "no_delta"
    };
    Ok(
        json!({"schema":"oev2.design-comparison-assessment/1","verdict":verdict,
        "counts":{"denominator":input.rules.len(),"comparable":improved+unchanged+regressed,
            "improved":improved,"unchanged":unchanged,"regressed":regressed,"unknown":unknown,
            "baseline_failed":baseline_failed,"candidate_failed":candidate_failed},
        "rows":rows,"observed_improvements":improved,"cost":{"status":"unknown","candidate_is_cheaper":false},
        "engineering_conclusion":"unknown","design_complete":false,"adopted":false,"method_promotion":"not_performed",
        "selection":if verdict=="improved"{"candidate_for_separate_method_evaluation"}else{"retain_baseline"},
        "scope":"A local metric under declared models, not physical acceptance or proven general method improvement",
        "transfer":"development_replay","model_calls":0}),
    )
}
