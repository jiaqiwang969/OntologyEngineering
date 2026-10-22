//! Mechanism assertions reuse the same CAD motion model and existing Project.
use crate::{CheckResult, engineering, project::Project};
use anyhow::{Result, ensure};
use oe_contracts::{EngineeringAssertion, EngineeringBasis, MechanismAssessment, ObjectIdentity};
use serde_json::{Value, json};
use std::collections::BTreeMap;
pub fn check(
    project: &Project,
    basis: &EngineeringBasis,
    a: &EngineeringAssertion,
    o: &ObjectIdentity,
) -> CheckResult {
    let mut details =
        json!({"subject":a.subject,"affected_decisions":engineering::affected(project,a)});
    let attempted = (|| -> Result<(String, Option<String>)> {
        let input: MechanismAssessment = serde_json::from_value(a.requirements.clone())?;
        input
            .validate()
            .map_err(|_| anyhow::anyhow!("mechanism_assessment_invalid"))?;
        let c = &input.drive;
        let diagnostic = oe_cad::motion::check_drive(c)?;
        details["motion"] = diagnostic.clone();
        ensure!(&c.object == o, "mechanism_object_or_source_mismatch");
        ensure!(
            a.subject == c.trajectory_id,
            "mechanism_trajectory_identity_mismatch"
        );
        let snapshot: BTreeMap<String, Value> = serde_json::from_value(input.condition_values)?;
        for fact in &basis.facts {
            ensure!(
                snapshot.get(&fact.property) == Some(engineering::known(basis, &fact.property)?),
                "mechanism_current_condition_changed:{}",
                fact.property
            );
        }
        ensure!(
            snapshot.len() == basis.facts.len(),
            "mechanism_condition_coverage_mismatch"
        );
        let sources: BTreeMap<String, String> = serde_json::from_value(input.source_versions)?;
        let required = basis
            .facts
            .iter()
            .flat_map(|f| f.source_refs.iter())
            .chain(a.source_refs.iter())
            .chain(c.source_refs.iter());
        for id in required {
            ensure!(
                sources.get(id).is_some() && sources.get(id) == project.evidence.get(id),
                "mechanism_source_stale_or_missing:{id}"
            );
        }
        for (id, version) in &sources {
            ensure!(
                project.evidence.get(id) == Some(version),
                "mechanism_extra_source_stale_or_missing:{id}"
            );
        }
        let conclusion = diagnostic["conclusion"]
            .as_str()
            .unwrap_or("unknown")
            .to_owned();
        let reasons = diagnostic["failures"]
            .as_array()
            .into_iter()
            .flatten()
            .chain(diagnostic["unknown"].as_array().into_iter().flatten())
            .filter_map(|f| f["reason"].as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join(",");
        Ok((
            conclusion.clone(),
            (conclusion != "pass")
                .then(|| format!("mechanism_declared_motion_requires_review:{reasons}")),
        ))
    })();
    let (conclusion, reason) =
        attempted.unwrap_or_else(|e| ("unknown".into(), Some(e.to_string())));
    engineering::result(
        &format!("engineering:{}", a.assertion_id),
        &conclusion,
        reason,
        details,
    )
}
