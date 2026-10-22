//! Current parametric intent lives in the existing Project condition envelope.
//! Hash identity alone cannot bind a proposed geometry to current parameters.
use crate::{CheckResult, engineering, project::Project};
use anyhow::{Context, Result, ensure};
use oe_contracts::{CenteredHoleRow, EngineeringAssertion, ObjectIdentity};
use serde_json::{Value, json};

pub fn check(project: &Project, row: &CenteredHoleRow) -> CheckResult {
    let property = format!("cad_row:{}", row.row_id);
    let checked = (|| -> Result<Value> {
        oe_cad::variant::plan(row)?;
        ensure!(
            project.objects.get(&row.object.object_id) == Some(&row.object.revision)
                && project
                    .evidence
                    .values()
                    .any(|sha| sha == &row.object.source_sha256),
            "cad_variant_object_changed"
        );
        ensure!(
            row.source_refs
                .iter()
                .all(|id| project.evidence.contains_key(id)),
            "cad_variant_source_unbound"
        );
        let basis = engineering::basis(project, &row.object)?
            .context("cad_variant_declared_row_missing")?;
        let fact = basis
            .facts
            .iter()
            .find(|f| f.property == property)
            .context("cad_variant_declared_row_missing")?;
        ensure!(
            fact.status == "declared" && !fact.value.is_null(),
            "cad_variant_declared_row_unknown"
        );
        let current: CenteredHoleRow = serde_json::from_value(fact.value.clone())
            .context("cad_variant_declared_row_invalid")?;
        let differences = oe_cad::variant::design_differences(row, &current)?;
        Ok(
            json!({"property":property,"differences":differences,"current_design":current,
            "proposed_sha256":oe_core::json::canonical_digest(&serde_json::to_value(row)?)?,
            "source_versions":row.source_refs.iter().map(|id|(id.clone(),json!(project.evidence[id]))).collect::<serde_json::Map<_,_>>() }),
        )
    })();
    match checked {
        Ok(details)
            if details["differences"]
                .as_array()
                .is_some_and(|d| d.is_empty()) =>
        {
            engineering::result(&property, "pass", None, details)
        }
        Ok(details) => engineering::result(
            &property,
            "violated",
            Some("cad_variant_design_intent_changed".into()),
            details,
        ),
        Err(error) => {
            // Legacy scalar fields are diagnostic clues, not a substitute for
            // the complete row, phase, frame, diameter and provenance binding.
            let legacy = engineering::basis(project, &row.object)
                .ok()
                .flatten()
                .and_then(|b| {
                    b.facts
                        .iter()
                        .find(|f| f.property == "plate_length")
                        .cloned()
                });
            let reason = error.to_string();
            let conclusion = if reason == "cad_variant_object_changed"
                || reason == "cad_variant_source_unbound"
            {
                "violated"
            } else {
                "unknown"
            };
            engineering::result(
                &property,
                conclusion,
                Some(reason),
                json!({"property":property,
                "proposed_length":row.length,"legacy_plate_length_clue":legacy,
                "required":"declared typed CenteredHoleRow in this object's EngineeringBasis; unchanged hashes alone are insufficient"}),
            )
        }
    }
}
pub(crate) fn assertion(
    project: &Project,
    object: &ObjectIdentity,
    a: &EngineeringAssertion,
) -> CheckResult {
    let attempted = (|| -> Result<CheckResult> {
        let row: CenteredHoleRow = serde_json::from_value(a.requirements.clone())?;
        ensure!(
            row.object == *object && row.row_id == a.subject,
            "cad_variant_assertion_identity_mismatch"
        );
        Ok(check(project, &row))
    })();
    let mut result = attempted.unwrap_or_else(|e| {
        engineering::result(&a.assertion_id, "unknown", Some(e.to_string()), json!({}))
    });
    result.id = format!("engineering:{}", a.assertion_id);
    result.result["details"]["affected_decisions"] = json!(engineering::affected(project, a));
    result
}
