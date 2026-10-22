//! Source-bound numeric acceptance in the existing Project condition basis.
//! Passing a declared numerical model is not engineering adoption.
use crate::{CheckResult, calculation, engineering, project::Project};
use anyhow::{Context, Result, ensure};
use oe_contracts::{
    EngineeringAssertion, EngineeringBasis, NumericAcceptance, NumericAcceptanceV2,
    NumericAssessment, NumericAssessmentV2, NumericObservation, ObjectIdentity, Quantity,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn valid(r: std::result::Result<(), Vec<String>>) -> Result<()> {
    r.map_err(|e| anyhow::anyhow!("numeric_contract_invalid:{}", e.join(",")))
}

/// Private numerical-field view, never a wire envelope. The original /2
/// requirement remains typed and is used for current-source equality. Reusing
/// the inclusive /1 numerical fields here does not change either wire protocol.
struct AssessmentView {
    common: NumericAssessment,
    strict_requirement: Option<NumericAcceptanceV2>,
    observation_reference: Option<ObjectIdentity>,
}
impl AssessmentView {
    fn parse(value: &Value) -> Result<Self> {
        if value["schema"] != "oev2.numeric-assessment/2" {
            let common: NumericAssessment =
                serde_json::from_value(value.clone()).context("numeric_assessment_invalid")?;
            valid(common.validate())?;
            return Ok(Self {
                common,
                strict_requirement: None,
                observation_reference: None,
            });
        }
        let input: NumericAssessmentV2 =
            serde_json::from_value(value.clone()).context("numeric_assessment_v2_invalid")?;
        valid(input.validate())?;
        valid(input.requirement.validate())?;
        if let Some(observation) = &input.observation {
            valid(observation.validate())?;
        }
        let r = &input.requirement;
        ensure!(
            r.schema == "oev2.numeric-acceptance/2",
            "numeric_v2_requirement_schema"
        );
        ensure!(r.relation == "strict_maximum", "numeric_v2_relation");
        let reference = input
            .observation
            .as_ref()
            .map(|o| o.reference_object.clone());
        let observation = input.observation.map(|o| NumericObservation {
            object: o.object,
            state: o.state,
            datum: o.datum,
            metric: o.metric,
            evidence_role: o.evidence_role,
            values: o.values,
            component_names: o.component_names,
            expanded_uncertainty: o.expanded_uncertainty,
            uncertainty_basis: o.uncertainty_basis,
            source_refs: o.source_refs,
        });
        let common = NumericAssessment {
            schema: "oev2.numeric-assessment/1".into(),
            requirement: NumericAcceptance {
                schema: "oev2.numeric-acceptance/1".into(),
                requirement_id: r.requirement_id.clone(),
                requirement_property: r.requirement_property.clone(),
                object: r.object.clone(),
                state: r.state.clone(),
                datum: r.datum.clone(),
                metric: r.metric.clone(),
                relation: "maximum".into(),
                lower: r.lower.clone(),
                upper: r.upper.clone(),
                target: r.target.clone(),
                tolerance: r.tolerance.clone(),
                definition_status: r.definition_status.clone(),
                coverage_status: r.coverage_status.clone(),
                evidence_role: r.evidence_role.clone(),
                source_refs: r.source_refs.clone(),
            },
            calculation: input.calculation,
            observation,
            condition_values: input.condition_values,
            source_versions: input.source_versions,
        };
        Ok(Self {
            common,
            strict_requirement: Some(input.requirement),
            observation_reference: reference,
        })
    }
    fn current_requirement(&self, current: &Value) -> Result<()> {
        if let Some(expected) = &self.strict_requirement {
            let actual: NumericAcceptanceV2 = serde_json::from_value(current.clone())
                .context("numeric_current_requirement_v2_invalid")?;
            valid(actual.validate())?;
            ensure!(&actual == expected, "numeric_current_requirement_changed");
        } else {
            let actual: NumericAcceptance = serde_json::from_value(current.clone())
                .context("numeric_current_requirement_invalid")?;
            valid(actual.validate())?;
            ensure!(
                actual == self.common.requirement,
                "numeric_current_requirement_changed"
            );
        }
        Ok(())
    }
}
fn q(q: &Quantity, unit: &str) -> Result<f64> {
    valid(q.validate())?;
    ensure!(q.unit == unit, "numeric_unit_mismatch");
    Ok(q.value)
}
fn bounds(r: &NumericAcceptance) -> Result<(f64, f64, String)> {
    valid(r.validate())?;
    let (low, high, unit) = match r.relation.as_str() {
        "maximum" => {
            ensure!(
                r.lower.is_none() && r.target.is_none() && r.tolerance.is_none(),
                "numeric_bounds_ambiguous"
            );
            let high = r.upper.as_ref().context("numeric_upper_missing")?;
            valid(high.validate())?;
            (f64::NEG_INFINITY, high.value, high.unit.clone())
        }
        "minimum" => {
            ensure!(
                r.upper.is_none() && r.target.is_none() && r.tolerance.is_none(),
                "numeric_bounds_ambiguous"
            );
            let low = r.lower.as_ref().context("numeric_lower_missing")?;
            valid(low.validate())?;
            (low.value, f64::INFINITY, low.unit.clone())
        }
        "range" => {
            ensure!(
                r.target.is_none() && r.tolerance.is_none(),
                "numeric_bounds_ambiguous"
            );
            let low = r.lower.as_ref().context("numeric_lower_missing")?;
            let high = r.upper.as_ref().context("numeric_upper_missing")?;
            valid(low.validate())?;
            (low.value, q(high, &low.unit)?, low.unit.clone())
        }
        "target_tolerance" => {
            ensure!(
                r.lower.is_none() && r.upper.is_none(),
                "numeric_bounds_ambiguous"
            );
            let target = r.target.as_ref().context("numeric_target_missing")?;
            valid(target.validate())?;
            let tolerance = q(
                r.tolerance.as_ref().context("numeric_tolerance_missing")?,
                &target.unit,
            )?;
            ensure!(tolerance >= 0.0, "numeric_tolerance_negative");
            let low = target.value - tolerance;
            let high = target.value + tolerance;
            ensure!(
                low.is_finite() && high.is_finite(),
                "numeric_bounds_nonfinite"
            );
            (low, high, target.unit.clone())
        }
        _ => anyhow::bail!("numeric_relation_unsupported"),
    };
    ensure!(low <= high, "numeric_bounds_reversed");
    if r.metric != "scalar" {
        ensure!(high >= 0.0, "numeric_nonnegative_metric_negative_bound");
    }
    Ok((low, high, unit))
}
fn interval(
    metric: &str,
    values: &[Quantity],
    uncertainty: f64,
    unit: &str,
) -> Result<(f64, f64, f64)> {
    ensure!(
        uncertainty.is_finite() && uncertainty >= 0.0,
        "numeric_uncertainty_invalid"
    );
    let value = match metric {
        "scalar" | "absolute" => {
            ensure!(values.len() == 1, "numeric_component_count_mismatch");
            let v = q(&values[0], unit)?;
            if metric == "absolute" { v.abs() } else { v }
        }
        "radial_2d" => {
            ensure!(values.len() == 2, "numeric_component_count_mismatch");
            q(&values[0], unit)?.hypot(q(&values[1], unit)?)
        }
        _ => anyhow::bail!("numeric_metric_unsupported"),
    };
    let lower = if metric == "scalar" {
        value - uncertainty
    } else {
        (value - uncertainty).max(0.0)
    };
    let upper = value + uncertainty;
    ensure!(
        value.is_finite() && lower.is_finite() && upper.is_finite(),
        "numeric_result_nonfinite"
    );
    Ok((value, lower, upper))
}

/// The input is a versioned assertion, not a second project state. Recompute
/// calculations; observations keep their acquisition conditions and source IDs.
pub fn check(
    project: &Project,
    basis: &EngineeringBasis,
    assertion: &EngineeringAssertion,
    object: &ObjectIdentity,
) -> CheckResult {
    let id = format!("engineering:{}", assertion.assertion_id);
    let affected = engineering::affected(project, assertion);
    let mut details = json!({"subject":assertion.subject,"kind":"numeric_acceptance","affected_decisions":affected,
        "model_applicability":{"execution":"not_run","conclusion":"unknown"},"arithmetic":{"execution":"not_run","conclusion":"unknown"},
        "numeric_acceptance":{"conclusion":"unknown"},"engineering_adoption":"not_established"});
    let attempted = (|| -> Result<(String, Option<String>)> {
        let view = AssessmentView::parse(&assertion.requirements)?;
        let input = &view.common;
        let strict = view.strict_requirement.is_some();
        let r = &input.requirement;
        valid(r.validate())?;
        let (lower, upper, unit) = bounds(r)?;
        // Do useful arithmetic even for a stale/unknown model. It remains a
        // diagnostic; subsequent binding failures cannot turn it into support.
        let arithmetic = if let Some(c) = &input.calculation {
            Some(calculation::calculate(c).unwrap_or_else(|e| json!({"execution":"failed","conclusion":"unknown","reason":e.to_string(),"engineering_adoption":"not_established"})))
        } else {
            None
        };
        if let Some(value) = &arithmetic {
            details["arithmetic"] = value.clone();
        }
        ensure!(
            input.calculation.is_some() || input.observation.is_some(),
            "numeric_result_missing"
        );
        ensure!(
            input.calculation.is_some() != input.observation.is_some(),
            "numeric_result_selection_ambiguous"
        );
        ensure!(
            assertion.subject == r.requirement_id,
            "numeric_requirement_identity_mismatch"
        );
        ensure!(
            &r.object == object,
            "numeric_object_version_or_source_mismatch"
        );
        ensure!(
            !r.source_refs.is_empty(),
            "numeric_requirement_source_missing"
        );
        let current = engineering::known(basis, &r.requirement_property)?;
        view.current_requirement(current)?;
        if let Some(original) = &view.strict_requirement {
            let reference = &original.reference_object;
            ensure!(
                reference.object_id != r.object.object_id,
                "numeric_reference_same_endpoint"
            );
            ensure!(
                reference.condition_scope == r.object.condition_scope,
                "numeric_reference_condition_scope_mismatch"
            );
            crate::project::context(project, reference).context("numeric_reference_not_current")?;
            if input.observation.is_some() {
                ensure!(
                    view.observation_reference.as_ref() == Some(reference),
                    "numeric_observation_reference_mismatch"
                );
            }
        }
        for (property, expected) in [("state", &r.state), ("datum", &r.datum)] {
            ensure!(
                engineering::known(basis, property)? == &json!(expected),
                "numeric_current_state_or_datum_mismatch:{property}"
            );
        }
        let snapshot: BTreeMap<String, Value> =
            serde_json::from_value(input.condition_values.clone())
                .context("numeric_condition_snapshot_invalid")?;
        let mut required = BTreeSet::new();
        for fact in &basis.facts {
            let current = engineering::known(basis, &fact.property)?;
            ensure!(
                snapshot.get(&fact.property) == Some(current),
                "numeric_current_condition_changed:{}",
                fact.property
            );
            required.extend(fact.source_refs.iter().cloned());
        }
        ensure!(
            snapshot.len() == basis.facts.len(),
            "numeric_condition_coverage_mismatch"
        );
        required.extend(r.source_refs.iter().cloned());
        required.extend(assertion.source_refs.iter().cloned());
        if let Some(c) = &input.calculation {
            required.extend(c.source_refs.iter().cloned());
        }
        if let Some(o) = &input.observation {
            required.extend(o.source_refs.iter().cloned());
        }
        let versions: BTreeMap<String, String> =
            serde_json::from_value(input.source_versions.clone())
                .context("numeric_source_versions_invalid")?;
        ensure!(
            !versions.is_empty() && versions.len() <= 256,
            "numeric_source_versions_missing"
        );
        for source in &required {
            let expected = versions
                .get(source)
                .with_context(|| format!("numeric_source_version_missing:{source}"))?;
            ensure!(
                project.evidence.get(source) == Some(expected),
                "numeric_source_version_stale:{source}"
            );
        }
        for (source, expected) in &versions {
            ensure!(
                project.evidence.get(source) == Some(expected),
                "numeric_source_version_stale:{source}"
            );
        }
        if let Some(original) = &view.strict_requirement {
            for endpoint in [&original.object, &original.reference_object] {
                ensure!(
                    versions.values().any(|v| v == &endpoint.source_sha256),
                    "numeric_endpoint_source_version_missing"
                );
            }
        }
        let (values, uncertainty, scope) = if let Some(c) = &input.calculation {
            ensure!(&c.object == object, "numeric_calculation_object_mismatch");
            ensure!(
                r.evidence_role == "calculated_model",
                "numeric_evidence_role_mismatch"
            );
            ensure!(
                r.metric != "radial_2d" || c.method == "radial-error-2d/1",
                "numeric_model_metric_mismatch"
            );
            ensure!(
                r.metric == "radial_2d" || c.method != "radial-error-2d/1",
                "numeric_model_metric_mismatch"
            );
            let applicability = engineering::calculation_check(project, c);
            details["model_applicability"] = serde_json::to_value(&applicability)?;
            let a = arithmetic.as_ref().context("numeric_calculation_missing")?;
            if applicability.conclusion != "pass" {
                return Ok((
                    applicability.conclusion.clone(),
                    Some("numeric_model_not_currently_supported".into()),
                ));
            }
            ensure!(
                a["execution"] == "completed",
                "numeric_calculation_incomplete"
            );
            let value: Quantity = serde_json::from_value(a["result"].clone())
                .context("numeric_calculation_result_invalid")?;
            // A computed radial magnitude is already a scalar norm. Do not
            // combine it with an invented second component.
            (vec![value], 0.0, "declared_model_only")
        } else {
            let o = input
                .observation
                .as_ref()
                .context("numeric_observation_missing")?;
            valid(o.validate())?;
            ensure!(
                o.object == r.object && o.state == r.state && o.datum == r.datum,
                "numeric_observation_identity_state_or_datum_mismatch"
            );
            ensure!(o.metric == r.metric, "numeric_observation_metric_mismatch");
            ensure!(
                o.evidence_role == r.evidence_role,
                "numeric_evidence_role_mismatch"
            );
            ensure!(
                !o.source_refs.is_empty(),
                "numeric_observation_source_missing"
            );
            ensure!(
                o.component_names.len() == o.values.len()
                    && o.component_names.iter().all(|s| !s.trim().is_empty())
                    && o.component_names.iter().collect::<BTreeSet<_>>().len()
                        == o.component_names.len(),
                "numeric_component_identity_invalid"
            );
            let u = q(
                o.expanded_uncertainty
                    .as_ref()
                    .context("numeric_uncertainty_missing")?,
                &unit,
            )?;
            ensure!(
                !o.uncertainty_basis.trim().is_empty(),
                "numeric_uncertainty_basis_missing"
            );
            details["arithmetic"] = json!({"execution":"completed","conclusion":"observed_values_only","values":o.values,"component_names":o.component_names,
                "uncertainty_basis":o.uncertainty_basis,"evidence_role":o.evidence_role});
            details["model_applicability"] = json!({"execution":"completed","conclusion":"pass","coverage":"declared acquisition object, state, datum, conditions and sources; physical calibration not certified"});
            (o.values.clone(), u, "source_bound_observation")
        };
        let metric = if input.calculation.is_some() && r.metric == "radial_2d" {
            "absolute"
        } else {
            &r.metric
        };
        let (value, low, high) = interval(metric, &values, uncertainty, &unit)?;
        let upper_inside = if strict { high < upper } else { high <= upper };
        let upper_outside = if strict { low >= upper } else { low > upper };
        let numeric = if low >= lower && upper_inside {
            "pass"
        } else if high < lower || upper_outside {
            "violated"
        } else {
            "unknown"
        };
        let reason = match numeric {
            "violated" => Some("numeric_requirement_exceeded".into()),
            "unknown" => Some("numeric_uncertainty_straddles_limit".into()),
            _ => None,
        };
        details["numeric_acceptance"] = json!({"conclusion":numeric,"value":value,"unit":unit,"interval":{"lower":low,"upper":high},
            "requirement_lower":if lower.is_finite(){Some(lower)}else{None},"requirement_upper":if upper.is_finite(){Some(upper)}else{None},
            "metric":r.metric,"relation":if strict {"strict_maximum"} else {&r.relation},"definition_status":r.definition_status,"coverage_status":r.coverage_status,"proof_scope":scope,
            "uncertainty":"numeric bounded/expanded input; confidence claim requires independent calibration evidence"});
        if let Some(original) = &view.strict_requirement {
            details["numeric_acceptance"]["assessment_protocol"] =
                json!("oev2.numeric-assessment/2");
            details["numeric_acceptance"]["requirement_protocol"] = json!(original.schema);
            details["numeric_acceptance"]["upper_inclusive"] = json!(false);
            details["numeric_acceptance"]["reference_object"] = json!(original.reference_object);
            details["numeric_acceptance"]["aggregation"] =
                json!("this_object_only; every other object requires its own check");
        }
        if r.definition_status != "confirmed" || r.coverage_status != "declared_complete" {
            details["numeric_acceptance"]["conditional_comparison"] = json!(numeric);
            details["numeric_acceptance"]["conclusion"] = json!("unknown");
            return Ok((
                "unknown".into(),
                Some("numeric_acceptance_definition_or_coverage_unconfirmed".into()),
            ));
        }
        Ok((numeric.into(), reason))
    })();
    let (conclusion, reason) =
        attempted.unwrap_or_else(|e| ("unknown".into(), Some(e.to_string())));
    if details["numeric_acceptance"]["conclusion"] == "unknown" && reason.is_some() {
        details["numeric_acceptance"]["reason"] = json!(reason);
    }
    details["support_status"] = json!(if conclusion == "pass" {
        "limited_numeric_support"
    } else {
        "requires_review"
    });
    engineering::result(&id, &conclusion, reason, details)
}
