//! Drawing obligations use the existing Project facts and dependency closure.
//! Declared geometry is recomputed; current source verification also reopens the
//! recorded job. Neither check grants physical or manufacturing adoption.
use crate::{CheckResult, engineering, project::Project};
use anyhow::{Context, Result, ensure};
use oe_contracts::{
    DrawingReferenceIntent, EngineeringAssertion, NativeDrawingObservation, ObjectIdentity,
};
use serde_json::{Value, json};
use std::path::Path;

pub fn check(project: &Project, intent: &DrawingReferenceIntent) -> CheckResult {
    let id = format!("drawing_reference:{}", intent.intent_id);
    let attempted = (|| -> Result<Value> {
        oe_cad::drawing::intent::validate(intent)?;
        ensure!(
            project.objects.get(&intent.object.object_id) == Some(&intent.object.revision)
                && project
                    .evidence
                    .values()
                    .any(|sha| sha == &intent.object.source_sha256),
            "drawing_reference_current_object_mismatch"
        );
        ensure!(
            intent
                .source_refs
                .iter()
                .all(|id| project.evidence.contains_key(id)),
            "drawing_reference_design_source_unbound"
        );
        let basis = engineering::basis(project, &intent.object)?
            .context("drawing_reference_current_intent_missing")?;
        let f = basis
            .facts
            .iter()
            .find(|f| f.property == id)
            .context("drawing_reference_current_intent_missing")?;
        ensure!(
            f.status == "declared" && !f.value.is_null(),
            "drawing_reference_current_intent_unknown"
        );
        let current: DrawingReferenceIntent = serde_json::from_value(f.value.clone())
            .context("drawing_reference_current_intent_invalid")?;
        oe_cad::drawing::intent::validate(&current)?;
        ensure!(
            current == *intent,
            "drawing_reference_design_content_changed"
        );
        let property = format!("native_drawing:{}", intent.intent_id);
        let f = basis
            .facts
            .iter()
            .find(|f| f.property == property)
            .context("drawing_reference_native_observation_missing")?;
        ensure!(
            f.status == "declared",
            "drawing_reference_native_observation_unknown"
        );
        let observation: NativeDrawingObservation = serde_json::from_value(f.value.clone())
            .context("drawing_reference_native_observation_invalid")?;
        observation
            .validate()
            .map_err(|_| anyhow::anyhow!("drawing_reference_native_observation_invalid"))?;
        ensure!(
            observation.object == intent.object,
            "drawing_reference_native_object_mismatch"
        );
        ensure!(
            project.evidence.get(&observation.native_result_ref)
                == Some(&observation.native_result_sha256)
                && f.source_refs.contains(&observation.native_result_ref),
            "drawing_reference_native_source_mismatch"
        );
        ensure!(
            intent
                .source_refs
                .iter()
                .all(|id| id != &observation.native_result_ref
                    && project.evidence[id] != observation.native_result_sha256),
            "drawing_reference_expectation_not_independent"
        );
        let mut result = oe_cad::drawing::intent::check_points(intent, &observation.part)?;
        result["native_result_ref"] = json!(observation.native_result_ref);
        result["native_result_sha256"] = json!(observation.native_result_sha256);
        result["cad_source_sha256"] = json!(intent.object.source_sha256);
        Ok(result)
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
                "drawing_reference_current_object_mismatch",
                "drawing_reference_design_source_unbound",
                "drawing_reference_design_content_changed",
                "drawing_reference_native_object_mismatch",
                "drawing_reference_native_source_mismatch",
                "drawing_reference_expectation_not_independent",
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
                json!({"intent_id":intent.intent_id,"object":intent.object,"engineering_adoption":"not_established"}),
            )
        }
    }
}

/// Separate from declared geometry so missing/changed job bytes cannot hide
/// the original point, topology or numerical diagnostics.
pub fn source_check(
    root: &oe_core::SkillRoot,
    project: &Project,
    intent: &DrawingReferenceIntent,
) -> CheckResult {
    let id = format!("drawing_native_source:{}", intent.intent_id);
    let details = json!({"intent_id":intent.intent_id,"object":intent.object,
        "proof_scope":"recorded completed native job and exact observed part; not a fresh OS observation",
        "engineering_adoption":"not_established"});
    if let Err(error) = oe_cad::drawing::intent::validate(intent) {
        return engineering::result(&id, "unknown", Some(error.to_string()), details);
    }
    source_observation_check(
        root,
        project,
        &intent.object,
        &format!("native_drawing:{}", intent.intent_id),
        &id,
        details,
    )
}

/// Reference annotations and drawing pages share one original-job verifier.
/// A pure declared geometry check remains separate from this byte projection.
pub(crate) fn source_observation_check(
    root: &oe_core::SkillRoot,
    project: &Project,
    object: &ObjectIdentity,
    property: &str,
    id: &str,
    mut details: Value,
) -> CheckResult {
    let attempted = (|| -> Result<()> {
        let basis = engineering::basis(project, object)?
            .context("drawing_reference_current_intent_missing")?;
        let f = basis
            .facts
            .iter()
            .find(|f| f.property == property)
            .context("drawing_reference_native_observation_missing")?;
        ensure!(
            f.status == "declared",
            "drawing_reference_native_observation_unknown"
        );
        let declared: NativeDrawingObservation = serde_json::from_value(f.value.clone())
            .context("drawing_reference_native_observation_invalid")?;
        declared
            .validate()
            .map_err(|_| anyhow::anyhow!("drawing_reference_native_observation_invalid"))?;
        ensure!(
            declared.object == *object,
            "drawing_reference_native_object_mismatch"
        );
        ensure!(
            declared.schema == "oev2.native-drawing-observation/2",
            "drawing_native_current_locator_missing"
        );
        let directory = declared
            .job_directory
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .context("drawing_native_current_locator_missing")?;
        let input = declared
            .part_input
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .context("drawing_native_current_input_missing")?;
        let job = root.private_path(Path::new(directory))?;
        let mut actual = oe_cad::drawing::intent::observe_part_bound(
            &job,
            input,
            object,
            &declared.native_result_ref,
        )?;
        details["native_result_sha256"] = json!(actual.native_result_sha256);
        details["actual_dimension_count"] = json!(
            actual.part["annotations"]["dimensions"]
                .as_array()
                .map(Vec::len)
        );
        details["declared_dimension_count"] = json!(
            declared.part["annotations"]["dimensions"]
                .as_array()
                .map(Vec::len)
        );
        ensure!(
            project.evidence.get(&declared.native_result_ref) == Some(&actual.native_result_sha256)
                && f.source_refs.contains(&declared.native_result_ref),
            "drawing_native_current_source_mismatch"
        );
        // Relative locators are useful after explicit receiving relocation. All
        // other DTO content must remain exactly the verified raw observation.
        actual.job_directory = declared.job_directory.clone();
        ensure!(
            actual == declared,
            "drawing_native_declared_observation_differs_from_job"
        );
        Ok(())
    })();
    match attempted {
        Ok(()) => engineering::result(id, "pass", None, details),
        Err(error) => {
            let reason = error.to_string();
            let conclusion = if [
                "drawing_native_current_source_mismatch",
                "drawing_native_declared_observation_differs_from_job",
                "drawing_reference_native_object_mismatch",
                "drawing_reference_cad_source_mismatch",
                "drawing_native_receipt_identity_mismatch",
                "drawing_native_completion_not_bound",
            ]
            .contains(&reason.as_str())
            {
                "violated"
            } else {
                "unknown"
            };
            engineering::result(id, conclusion, Some(reason), details)
        }
    }
}

pub(crate) fn source_assertion(
    root: &oe_core::SkillRoot,
    project: &Project,
    object: &ObjectIdentity,
    assertion: &EngineeringAssertion,
) -> CheckResult {
    let attempted = (|| -> Result<CheckResult> {
        let intent: DrawingReferenceIntent = serde_json::from_value(assertion.requirements.clone())
            .context("drawing_reference_intent_missing_or_invalid")?;
        ensure!(
            intent.object == *object && intent.intent_id == assertion.subject,
            "drawing_reference_assertion_identity_mismatch"
        );
        Ok(source_check(root, project, &intent))
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
    result.coverage = vec![
        "recorded native receipt, complete job byte identities and declared observation".into(),
    ];
    result.result["details"]["affected_decisions"] =
        json!(engineering::affected(project, assertion));
    result
}
pub(crate) fn assertion(
    project: &Project,
    object: &ObjectIdentity,
    assertion: &EngineeringAssertion,
) -> CheckResult {
    let attempted = (|| -> Result<CheckResult> {
        // A recorded but unfilled obligation is not a malformed known intent.
        // Preserve the missing inputs without inventing coordinates or approval.
        if assertion.requirements.is_null() {
            return Ok(engineering::result(
                &assertion.assertion_id,
                "unknown",
                Some("drawing_reference_intent_missing".into()),
                json!({"intent_id":assertion.subject,"object":object,"required_inputs":[
                    "independent_design_reference_points", "current_cad_object_and_source",
                    "coordinate_frame_and_unit", "point_and_parent_modes_with_sdk_basis",
                    "diagnostic_tolerance_and_proof_scope", "source_bound_native_observation"
                ]}),
            ));
        }
        let intent: DrawingReferenceIntent =
            serde_json::from_value(assertion.requirements.clone())?;
        ensure!(
            intent.object == *object && intent.intent_id == assertion.subject,
            "drawing_reference_assertion_identity_mismatch"
        );
        Ok(check(project, &intent))
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
