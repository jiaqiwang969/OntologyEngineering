//! Current Project obligations for one source-bound prototype drawing page.
//! A geometrically current page does not establish manufacturing readiness.
use crate::{CheckResult, engineering, project::Project};
use anyhow::{Context, Result, ensure};
use oe_contracts::{
    EngineeringAssertion, NativeDraftingPlan, NativeDrawingObservation, ObjectIdentity,
};
use serde_json::{Value, json};

pub fn check(project: &Project, plan: &NativeDraftingPlan) -> CheckResult {
    let id = format!("drafting_sheet:{}", plan.sheet_name);
    let attempted = (|| -> Result<Value> {
        oe_cad::drawing::drafting::validate(plan)?;
        ensure!(
            project.objects.get(&plan.object.object_id) == Some(&plan.object.revision)
                && project
                    .evidence
                    .values()
                    .any(|sha| sha == &plan.object.source_sha256),
            "drafting_current_object_mismatch"
        );
        ensure!(
            plan.source_refs
                .iter()
                .all(|id| project.evidence.contains_key(id)),
            "drafting_design_source_unbound"
        );
        let basis =
            engineering::basis(project, &plan.object)?.context("drafting_current_plan_missing")?;
        let property = format!("drafting_plan:{}", plan.sheet_name);
        let f = basis
            .facts
            .iter()
            .find(|f| f.property == property)
            .context("drafting_current_plan_missing")?;
        ensure!(
            f.status == "declared" && !f.value.is_null(),
            "drafting_current_plan_unknown"
        );
        let current: NativeDraftingPlan =
            serde_json::from_value(f.value.clone()).context("drafting_current_plan_invalid")?;
        oe_cad::drawing::drafting::validate(&current)?;
        ensure!(current == *plan, "drafting_design_content_changed");
        let property = format!("native_drafting:{}", plan.sheet_name);
        let f = basis
            .facts
            .iter()
            .find(|f| f.property == property)
            .context("drafting_native_observation_missing")?;
        ensure!(
            f.status == "declared",
            "drafting_native_observation_unknown"
        );
        let observation: NativeDrawingObservation = serde_json::from_value(f.value.clone())
            .context("drafting_native_observation_invalid")?;
        observation
            .validate()
            .map_err(|_| anyhow::anyhow!("drafting_native_observation_invalid"))?;
        ensure!(
            observation.object == plan.object,
            "drafting_native_object_mismatch"
        );
        ensure!(
            project.evidence.get(&observation.native_result_ref)
                == Some(&observation.native_result_sha256)
                && f.source_refs.contains(&observation.native_result_ref),
            "drafting_native_source_mismatch"
        );
        ensure!(
            plan.source_refs
                .iter()
                .all(|id| id != &observation.native_result_ref
                    && project.evidence[id] != observation.native_result_sha256),
            "drafting_expectation_not_independent"
        );
        let mut checked = oe_cad::drawing::drafting::check(plan, &observation.part)?;
        checked["native_result_ref"] = json!(observation.native_result_ref);
        checked["native_result_sha256"] = json!(observation.native_result_sha256);
        checked["cad_source_sha256"] = json!(plan.object.source_sha256);
        Ok(checked)
    })();
    match attempted {
        Ok(value) => engineering::result(
            &id,
            value["conclusion"].as_str().unwrap_or("unknown"),
            value["reason"].as_str().map(str::to_owned),
            value.clone(),
        ),
        Err(error) => {
            let reason = error.to_string();
            let conclusion = if [
                "drafting_current_object_mismatch",
                "drafting_design_source_unbound",
                "drafting_design_content_changed",
                "drafting_native_object_mismatch",
                "drafting_native_source_mismatch",
                "drafting_expectation_not_independent",
            ]
            .contains(&reason.as_str())
            {
                "violated"
            } else {
                "unknown"
            };
            engineering::result(
                &id,
                conclusion,
                Some(reason),
                json!({"sheet_name":plan.sheet_name,
                "object":plan.object,"manufacturing_drawing":"not_established","engineering_adoption":"not_established"}),
            )
        }
    }
}

pub fn source_check(
    root: &oe_core::SkillRoot,
    project: &Project,
    plan: &NativeDraftingPlan,
) -> CheckResult {
    let id = format!("drafting_native_source:{}", plan.sheet_name);
    let details = json!({"sheet_name":plan.sheet_name,"object":plan.object,
        "proof_scope":"recorded completed native job and exact observed part; not a fresh OS observation",
        "manufacturing_drawing":"not_established","engineering_adoption":"not_established"});
    if let Err(error) = oe_cad::drawing::drafting::validate(plan) {
        return engineering::result(&id, "unknown", Some(error.to_string()), details);
    }
    crate::drawing_intent::source_observation_check(
        root,
        project,
        &plan.object,
        &format!("native_drafting:{}", plan.sheet_name),
        &id,
        details,
    )
}

pub(crate) fn assertion(
    project: &Project,
    object: &ObjectIdentity,
    assertion: &EngineeringAssertion,
) -> CheckResult {
    let attempted = (|| -> Result<CheckResult> {
        if assertion.requirements.is_null() {
            return Ok(engineering::result(
                &assertion.assertion_id,
                "unknown",
                Some("drafting_current_plan_missing".into()),
                json!({"sheet_name":assertion.subject,
                    "object":object,"required_inputs":["independent_prototype_drawing_plan",
                        "current_cad_object_and_source","page_unit_size_and_projection",
                        "named_model_view_scale_and_sheet_position","source_bound_native_observation"],
                    "manufacturing_drawing":"not_established"}),
            ));
        }
        let plan: NativeDraftingPlan = serde_json::from_value(assertion.requirements.clone())?;
        ensure!(
            plan.object == *object && plan.sheet_name == assertion.subject,
            "drafting_assertion_identity_mismatch"
        );
        Ok(check(project, &plan))
    })();
    let mut result = attempted.unwrap_or_else(|error| {
        engineering::result(
            &assertion.assertion_id,
            "unknown",
            Some(error.to_string()),
            json!({}),
        )
    });
    result.id = format!("engineering:{}", assertion.assertion_id);
    result.result["details"]["affected_decisions"] =
        json!(engineering::affected(project, assertion));
    result
}

pub(crate) fn source_assertion(
    root: &oe_core::SkillRoot,
    project: &Project,
    object: &ObjectIdentity,
    assertion: &EngineeringAssertion,
) -> CheckResult {
    let attempted = (|| -> Result<CheckResult> {
        let plan: NativeDraftingPlan = serde_json::from_value(assertion.requirements.clone())
            .context("drafting_current_plan_missing_or_invalid")?;
        ensure!(
            plan.object == *object && plan.sheet_name == assertion.subject,
            "drafting_assertion_identity_mismatch"
        );
        Ok(source_check(root, project, &plan))
    })();
    let mut result = attempted.unwrap_or_else(|error| {
        engineering::result(
            &assertion.assertion_id,
            "unknown",
            Some(error.to_string()),
            json!({}),
        )
    });
    result.id = format!("engineering_native_source:{}", assertion.assertion_id);
    result.coverage = vec!["recorded native receipt, complete job byte identities and declared drawing page observation".into()];
    result.result["details"]["affected_decisions"] =
        json!(engineering::affected(project, assertion));
    result
}
